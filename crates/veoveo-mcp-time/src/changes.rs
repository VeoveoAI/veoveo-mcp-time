//! Change notifications from Time's tables. Triggers publish each row change on the
//! `time` channel; every replica's watcher evicts the tenant's authority cache, wakes the
//! due loop and tells that tenant's subscribers which resources changed.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
};

use futures::StreamExt;
use tokio::sync::Notify;
use veoveo_hub_sdk::SubscriptionHub;
use veoveo_store::{Change, ChangeSignal, ChangeStream, Channel, ServerSchema, TenantId};
use veoveo_types::ServerSlug;

use crate::{
    contract::{EpochId, EventId, TimeResource},
    registry::AuthorityRegistry,
    uris,
};

/// Time's server name, which names its schema, roles and channel.
pub fn server() -> ServerSlug {
    ServerSlug::parse("time").expect("Time's slug")
}

/// The schema `time` that holds Time's tables.
pub fn schema() -> ServerSchema {
    ServerSchema::new(&server()).expect("Time's slug names a server schema")
}

/// The channel on which Time's tables notify changes: its schema's name.
pub fn channel() -> Channel {
    schema().channel()
}

/// Time's tables as notifications name them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
enum TimeTable {
    #[vocabulary(rename = "source")]
    Source,
    #[vocabulary(rename = "authority_release")]
    AuthorityRelease,
    #[vocabulary(rename = "acquisition")]
    Acquisition,
    #[vocabulary(rename = "active_authority")]
    ActiveAuthority,
    #[vocabulary(rename = "calendar_version")]
    CalendarVersion,
    #[vocabulary(rename = "epoch")]
    Epoch,
    #[vocabulary(rename = "event")]
    Event,
    #[vocabulary(rename = "clock_policy")]
    ClockPolicy,
}

impl TimeTable {
    /// Rows that select or describe the tenant's loaded authority.
    fn affects_authority(self) -> bool {
        matches!(
            self,
            Self::AuthorityRelease | Self::Acquisition | Self::ActiveAuthority
        )
    }

    /// The subscribable resources whose contents a change of `key` may alter.
    fn updated_resources(self, key: Option<&str>) -> Vec<String> {
        match self {
            Self::Event => {
                let mut changed = vec![uris::EVENTS_URI.to_owned()];
                changed.extend(
                    key.and_then(|key| EventId::parse(key).ok())
                        .map(|id| TimeResource::Event(id).to_string()),
                );
                changed
            }
            Self::Epoch => {
                let mut changed = vec![uris::EPOCHS_URI.to_owned()];
                changed.extend(
                    key.and_then(|key| EpochId::parse(key).ok())
                        .map(|id| TimeResource::Epoch(id).to_string()),
                );
                changed
            }
            Self::CalendarVersion => vec![uris::CALENDARS_URI.to_owned()],
            Self::ActiveAuthority => vec![
                uris::AUTHORITIES_CURRENT_URI.to_owned(),
                uris::CLOCK_CURRENT_URI.to_owned(),
            ],
            Self::ClockPolicy => vec![uris::CLOCK_CURRENT_URI.to_owned()],
            Self::Source | Self::AuthorityRelease | Self::Acquisition => Vec::new(),
        }
    }
}

/// Resource update hubs by tenant. A `subscriptions/listen` request listens to its
/// caller's tenant hub, so a subscriber hears only about its own tenant's resources.
/// The map keeps a hub while a listener holds it.
#[derive(Default)]
pub struct TenantSubscriptions {
    hubs: Mutex<BTreeMap<TenantId, Weak<SubscriptionHub>>>,
}

impl TenantSubscriptions {
    /// The tenant's hub, shared by its current listeners.
    pub fn hub(&self, tenant: TenantId) -> Arc<SubscriptionHub> {
        let mut hubs = self.hubs.lock().expect("subscription hubs lock");
        hubs.retain(|_, hub| hub.strong_count() > 0);
        if let Some(hub) = hubs.get(&tenant).and_then(Weak::upgrade) {
            return hub;
        }
        let hub = Arc::new(SubscriptionHub::new());
        hubs.insert(tenant, Arc::downgrade(&hub));
        hub
    }

    fn listening(&self, tenant: TenantId) -> Option<Arc<SubscriptionHub>> {
        self.hubs
            .lock()
            .expect("subscription hubs lock")
            .get(&tenant)
            .and_then(Weak::upgrade)
    }

    fn all(&self) -> Vec<Arc<SubscriptionHub>> {
        self.hubs
            .lock()
            .expect("subscription hubs lock")
            .values()
            .filter_map(Weak::upgrade)
            .collect()
    }

    /// Tell the tenant's listeners that `uri` changed.
    pub async fn notify(&self, tenant: TenantId, uri: impl Into<String>) {
        if let Some(hub) = self.listening(tenant) {
            hub.notify_resource_updated(uri).await;
        }
    }

    /// Ask every listener to reread each resource it watches.
    pub async fn reconcile(&self) {
        for hub in self.all() {
            hub.notify_resource_contents_changed().await;
        }
    }
}

/// Applies the `time` channel's signals to one process.
#[derive(Clone)]
pub struct ChangeWatcher {
    pub subscriptions: Arc<TenantSubscriptions>,
    pub authorities: AuthorityRegistry,
    /// Wakes the due loop to reread the earliest scheduled event.
    pub due: Arc<Notify>,
}

impl ChangeWatcher {
    /// Apply signals until the stream ends. A lost connection reconnects inside the
    /// stream, which then asks for reconciliation because PostgreSQL does not replay
    /// notifications sent while it was disconnected.
    pub async fn run(self, mut changes: ChangeStream) {
        while let Some(signal) = changes.next().await {
            match signal {
                Ok(signal) => self.apply(signal).await,
                Err(error) => tracing::warn!(%error, "Time's change listener lost its connection"),
            }
        }
    }

    pub async fn apply(&self, signal: ChangeSignal) {
        match signal {
            ChangeSignal::Reconcile => {
                self.authorities.invalidate().await;
                self.subscriptions.reconcile().await;
                self.due.notify_one();
            }
            ChangeSignal::Changed(change) => self.changed(change).await,
        }
    }

    async fn changed(&self, change: Change) {
        let Ok(table) = change.table.parse::<TimeTable>() else {
            tracing::warn!(table = %change.table, "change notification names an unknown Time table");
            return;
        };
        if table == TimeTable::Event {
            self.due.notify_one();
        }
        let Some(tenant) = change.tenant else {
            tracing::warn!(table = %change.table, "change notification names no tenant");
            return;
        };
        if table.affects_authority() {
            self.authorities.invalidate_tenant(tenant).await;
        }
        for uri in table.updated_resources(change.key.as_deref()) {
            self.subscriptions.notify(tenant, uri).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_names_admit_only_time_tables() {
        for &table in TimeTable::ALL {
            assert_eq!(table.as_str().parse::<TimeTable>().unwrap(), table);
        }
        assert!("instance".parse::<TimeTable>().is_err());
        assert!("time.event".parse::<TimeTable>().is_err());
    }

    #[test]
    fn event_changes_update_the_index_and_the_event() {
        let key = format!("event-{}", uuid::Uuid::now_v7());
        assert_eq!(
            TimeTable::Event.updated_resources(Some(&key)),
            [uris::EVENTS_URI.to_owned(), format!("time://events/{key}")]
        );
        assert_eq!(
            TimeTable::Event.updated_resources(None),
            [uris::EVENTS_URI.to_owned()]
        );
        assert!(TimeTable::Source.updated_resources(Some("x")).is_empty());
    }

    #[tokio::test]
    async fn tenant_hubs_live_while_a_listener_holds_them() {
        let subscriptions = TenantSubscriptions::default();
        let tenant = veoveo_store::deterministic_tenant_id("hub").unwrap();
        let other = veoveo_store::deterministic_tenant_id("other").unwrap();
        let hub = subscriptions.hub(tenant);
        assert!(Arc::ptr_eq(&hub, &subscriptions.hub(tenant)));
        let mut updates = hub.listen();
        subscriptions.notify(other, uris::EVENTS_URI).await;
        subscriptions.notify(tenant, uris::EPOCHS_URI).await;
        match updates.recv().await.unwrap() {
            veoveo_hub_sdk::ResourceUpdate::Uri(uri) => assert_eq!(uri, uris::EPOCHS_URI),
            veoveo_hub_sdk::ResourceUpdate::Reconcile => panic!("expected one URI"),
        }
        drop(updates);
        drop(hub);
        assert!(subscriptions.listening(tenant).is_none());
    }
}
