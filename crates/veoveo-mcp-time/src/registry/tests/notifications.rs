//! Row triggers notify the `time` channel; the change watcher evicts the tenant's
//! authority context and tells that tenant's subscribers which resources changed.
use super::*;
use crate::changes::{ChangeWatcher, TenantSubscriptions, channel};
use futures::StreamExt;
use tokio::sync::Notify;
use veoveo_mcp_contract::ResourceUpdate;
use veoveo_platform_store::{ChangeOperation, ChangeSignal, ChangeStream};

const WAIT: Duration = Duration::from_secs(10);

/// The next signal on `changes` that satisfies `wanted`.
async fn next_signal(
    changes: &mut ChangeStream,
    wanted: impl Fn(&ChangeSignal) -> bool,
) -> ChangeSignal {
    tokio::time::timeout(WAIT, async {
        loop {
            let signal = changes.next().await.unwrap().unwrap();
            if wanted(&signal) {
                return signal;
            }
        }
    })
    .await
    .expect("the change listener delivered the signal")
}

fn later() -> chrono::DateTime<Utc> {
    Utc::now() + chrono::TimeDelta::days(1)
}

async fn event(
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    due: TimeInstant,
    utc: chrono::DateTime<Utc>,
) -> Event {
    catalog
        .create_event(
            scope,
            Event {
                event_id: EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap(),
                name: "notified".into(),
                due,
                state: EventState::Scheduled,
                record_version: TimeVersion::FIRST,
            },
            utc,
            Uuid::now_v7().to_string(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn authority_changes_evict_the_tenant_context_without_becoming_a_freshness_dependency() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let registry = files.registry();
        let writer = TimeCatalog::new(db.a.pool().clone());
        let reader = TimeCatalog::new(db.b.pool().clone());
        let owner = scope(&db.a, "registry-observation").await;
        let tenant = owner.identity.tenant_id;
        let watcher = ChangeWatcher {
            subscriptions: Arc::new(TenantSubscriptions::default()),
            authorities: registry.clone(),
            due: Arc::new(Notify::new()),
        };
        let mut changes = db.b.listen(vec![channel()]);
        assert_eq!(
            next_signal(&mut changes, |_| true).await,
            ChangeSignal::Reconcile
        );
        let bootstrap = registry.authority_engine(&reader, &owner).await.unwrap();
        let (first, _) = stage(
            &writer,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        activate(&writer, &registry, &owner, &first, TimeWriteGuard::Absent).await;
        let pointer = next_signal(&mut changes, |signal| {
            matches!(signal, ChangeSignal::Changed(change) if change.table == "active_authority")
        })
        .await;
        let ChangeSignal::Changed(change) = &pointer else {
            unreachable!()
        };
        assert_eq!(change.channel, channel());
        assert_eq!(change.operation, ChangeOperation::Insert);
        assert_eq!(change.tenant, Some(tenant));
        assert_eq!(change.key.as_deref(), Some("leap_seconds"));

        // Request-time selection does not wait for the notification.
        let first_engine = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_ne!(
            first_engine.authority().binding(),
            bootstrap.authority().binding()
        );
        let cached = registry.authority_engine(&reader, &owner).await.unwrap();
        assert!(Arc::ptr_eq(
            first_engine.authority().leap_seconds(),
            cached.authority().leap_seconds()
        ));
        // The tenant's authority change evicts its loaded context.
        watcher.apply(pointer).await;
        let reloaded = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            reloaded.authority().effective(),
            first_engine.authority().effective()
        );
        assert!(!Arc::ptr_eq(
            reloaded.authority().leap_seconds(),
            first_engine.authority().leap_seconds()
        ));
        // A reconnect names no tenant and evicts every context.
        watcher.apply(ChangeSignal::Reconcile).await;
        let reconciled = registry.authority_engine(&reader, &owner).await.unwrap();
        assert!(!Arc::ptr_eq(
            reconciled.authority().leap_seconds(),
            reloaded.authority().leap_seconds()
        ));

        let (next, _) = stage(
            &writer,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.next_leaps,
        )
        .await;
        activate(
            &writer,
            &registry,
            &owner,
            &next,
            TimeWriteGuard::Existing(TimeVersion::FIRST),
        )
        .await;
        let next_engine = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            next_engine.authority().binding().leap_seconds_release_id(),
            &next.release_id
        );
        assert_eq!(
            resolved(&next_engine).tai_seconds_since_1970,
            resolved(&first_engine).tai_seconds_since_1970 + 1
        );
    })
    .await
    .expect("authority observation qualification exceeded 90 seconds");
}

#[tokio::test]
async fn subscribers_hear_only_their_tenants_resource_changes() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.a.pool().clone());
        let owner = scope(&db.a, "notified-tenant").await;
        let other = scope(&db.a, "other-tenant").await;
        let subscriptions = Arc::new(TenantSubscriptions::default());
        let watcher = ChangeWatcher {
            subscriptions: subscriptions.clone(),
            authorities: files.registry(),
            due: Arc::new(Notify::new()),
        };
        // A listener holds its tenant's hub for as long as it listens.
        let hub = subscriptions.hub(owner.identity.tenant_id);
        let mut updates = hub.listen();
        let changes = db.b.listen(vec![channel()]);
        let watching = tokio::spawn(watcher.run(changes));
        // The watcher reconciles once its listener is subscribed.
        let reconciled = tokio::time::timeout(WAIT, updates.recv()).await.unwrap();
        assert!(matches!(reconciled, Ok(ResourceUpdate::Reconcile)));

        let instant = resolved(&TemporalEngine::new(files.bootstrap.clone()));
        let foreign = event(&catalog, &other, instant.clone(), later()).await;
        let own = event(&catalog, &owner, instant, later()).await;
        let mut heard = Vec::new();
        while heard.len() < 2 {
            match tokio::time::timeout(WAIT, updates.recv())
                .await
                .unwrap()
                .unwrap()
            {
                ResourceUpdate::Uri(uri) => heard.push(uri),
                ResourceUpdate::Reconcile => panic!("no reconnect is expected"),
            }
        }
        assert_eq!(
            heard,
            [
                crate::uris::EVENTS_URI.to_owned(),
                format!("time://events/{}", own.event_id)
            ]
        );
        assert!(
            heard
                .iter()
                .all(|uri| !uri.contains(foreign.event_id.as_str()))
        );

        // The other tenant's later changes still reach nobody here.
        catalog
            .cancel_event(&other, &foreign.event_id, TimeVersion::FIRST)
            .await
            .unwrap();
        catalog
            .cancel_event(&owner, &own.event_id, TimeVersion::FIRST)
            .await
            .unwrap();
        let mut heard = Vec::new();
        while heard.len() < 2 {
            match tokio::time::timeout(WAIT, updates.recv())
                .await
                .unwrap()
                .unwrap()
            {
                ResourceUpdate::Uri(uri) => heard.push(uri),
                ResourceUpdate::Reconcile => panic!("no reconnect is expected"),
            }
        }
        assert!(
            heard
                .iter()
                .all(|uri| !uri.contains(foreign.event_id.as_str()))
        );
        assert!(heard.contains(&format!("time://events/{}", own.event_id)));
        watching.abort();
    })
    .await
    .expect("tenant notification qualification exceeded 90 seconds");
}
