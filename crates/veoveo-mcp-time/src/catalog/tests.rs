use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::catalog::TimeCompletion;
use crate::persistence::{TimeEpochDraft, TimeEventState as StoredEventState, TimePersistence};
use crate::{
    AuthorityBinding, AuthorityReleaseId, CalendarId, Epoch, EpochId, Event, EventId, EventState,
    TimeInstant,
    catalog::{TimeAccessContext, TimeCatalog},
};
use veoveo_store::{PlatformStore, PrincipalKind};

mod admin_pages;
mod metadata;
mod numeric;
mod precision;

fn selection<C: veoveo_types::Collection<Limit = veoveo_types::PageSize<100>>>()
-> veoveo_hub_sdk::definition::CollectionRequest<C>
where
    C::Filters: Default,
{
    veoveo_hub_sdk::definition::CollectionRequest::new(
        C::Filters::default(),
        veoveo_types::PageSize::new(100).unwrap(),
    )
}
fn event_selection(
    state: Option<EventState>,
) -> veoveo_hub_sdk::definition::CollectionRequest<crate::Events> {
    veoveo_hub_sdk::definition::CollectionRequest::new(
        crate::EventFilters { state },
        veoveo_types::PageSize::new(100).unwrap(),
    )
}

async fn scope(store: &PlatformStore, tenant: &str, owner: &str) -> TimeAccessContext {
    TimeAccessContext {
        work_context: "fixture-context".parse().unwrap(),
        identity: store
            .ensure_identity(
                tenant,
                owner,
                "https://time.example",
                owner,
                PrincipalKind::User,
            )
            .await
            .unwrap(),
    }
}

fn instant() -> TimeInstant {
    TimeInstant {
        tai_seconds_since_1970: 2_000_000_000,
        nanosecond: crate::SubsecondNanoseconds::new(17).unwrap(),
        uncertainty: crate::NanosecondBound::ZERO,
        authority: AuthorityBinding::new(
            AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
            AuthorityReleaseId::parse("time-release-leaps").unwrap(),
        )
        .unwrap(),
    }
}

/// A UTC due instant far enough ahead that no event of a catalog test becomes due.
fn later() -> DateTime<Utc> {
    Utc::now() + chrono::TimeDelta::days(1)
}

async fn event(catalog: &TimeCatalog, owner: &TimeAccessContext, key: &str) -> Event {
    catalog
        .create_event(
            owner,
            Event {
                event_id: EventId::parse(key).unwrap(),
                name: key.into(),
                due: instant(),
                state: EventState::Scheduled,
                record_version: crate::TimeVersion::new(1).unwrap(),
            },
            later(),
            key.into(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn sql_filters_owner_tenant_latest_version_and_completion_before_limit() {
    // This is native Store qualification; no reference installation or GPU is required.
    let db = crate::test_database::TimeDb::new().await;
    tokio::time::timeout(Duration::from_secs(90), async {
        let owner = scope(&db.a, "time-query-test", "owner").await;
        let peer = scope(&db.a, "time-query-test", "peer").await;
        let foreign = scope(&db.a, "time-query-other", "owner").await;
        let catalog = TimeCatalog::new(db.b.pool().clone());

        for i in 0..102 {
            event(
                &catalog,
                &peer,
                &format!("event-00000000-0000-7000-8000-{i:012x}"),
            )
            .await;
            event(
                &catalog,
                &owner,
                &format!("event-ffffffff-ffff-7000-8000-{i:012x}"),
            )
            .await;
        }
        let foreign_event = event(
            &catalog,
            &foreign,
            "event-00000000-0000-7000-9000-000000000000",
        )
        .await;
        let peer_event = EventId::parse("event-00000000-0000-7000-8000-000000000000").unwrap();
        assert!(catalog.event(&owner, &peer_event).await.unwrap().is_none());
        assert!(
            catalog
                .event(&owner, &foreign_event.event_id)
                .await
                .unwrap()
                .is_none()
        );
        let own = TimePersistence::new(db.a.pool().clone())
            .list_time_events(&owner.identity, None, None, 101)
            .await
            .unwrap();
        assert_eq!(own.len(), 101);
        assert!(
            own.iter()
                .all(|row| row.owner == owner.identity.principal_id)
        );
        let page = catalog
            .events_page(&owner, &selection::<crate::Events>())
            .await
            .unwrap();
        assert_eq!(page.limit().get(), 100);
        assert_eq!(page.items().len(), 100);
        assert!(page.next_cursor().is_some());
        let cursor: veoveo_types::CursorToken<crate::Events> =
            serde_json::from_value(serde_json::to_value(page.next_cursor().unwrap()).unwrap())
                .unwrap();
        let next = catalog
            .events_page(
                &owner,
                &selection::<crate::Events>().continuing(cursor.clone()),
            )
            .await
            .unwrap();
        assert_eq!(next.items().len(), 2);
        assert!(next.next_cursor().is_none());
        assert_eq!(
            next.items()[0].event_id.as_str(),
            "event-ffffffff-ffff-7000-8000-000000000064"
        );
        assert!(
            catalog
                .events_page(
                    &foreign,
                    &selection::<crate::Events>().continuing(cursor.clone())
                )
                .await
                .is_err()
        );
        // A persistence caller cannot bypass the same owner predicate on a transition.
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .transition_time_event(
                    &owner.identity,
                    &peer_event,
                    crate::TimeVersion::FIRST,
                    StoredEventState::Cancelled,
                    serde_json::json!({}),
                )
                .await
                .is_err()
        );
        assert_eq!(
            catalog
                .event(&peer, &peer_event)
                .await
                .unwrap()
                .unwrap()
                .state,
            EventState::Scheduled
        );
        let own_id = EventId::parse("event-ffffffff-ffff-7000-8000-000000000000").unwrap();
        assert_eq!(
            catalog
                .cancel_event(&owner, &own_id, crate::TimeVersion::FIRST)
                .await
                .unwrap()
                .state,
            EventState::Cancelled
        );
        assert!(
            catalog
                .cancel_event(&owner, &own_id, crate::TimeVersion::FIRST)
                .await
                .is_err()
        );

        let matches = TimePersistence::new(db.a.pool().clone())
            .complete_time_values(&owner.identity, TimeCompletion::EventId, "", 101)
            .await
            .unwrap();
        assert_eq!(matches.len(), 101);
        assert_eq!(
            matches.first().unwrap(),
            "event-ffffffff-ffff-7000-8000-000000000000"
        );
        assert_eq!(
            matches.last().unwrap(),
            "event-ffffffff-ffff-7000-8000-000000000064"
        );
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(
                    &owner.identity,
                    TimeCompletion::EventId,
                    "EVENT-FFFFFFFF-FFFF-7000-8000-000000000065",
                    101
                )
                .await
                .unwrap(),
            vec!["event-ffffffff-ffff-7000-8000-000000000065"]
        );
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(
                    &owner.identity,
                    TimeCompletion::EventId,
                    "event-ffffffff-ffff-7000-8000-00000000006",
                    101
                )
                .await
                .unwrap(),
            (0x60..=0x65)
                .map(|i| format!("event-ffffffff-ffff-7000-8000-{i:012x}"))
                .collect::<Vec<_>>()
        );
        // Completion matches a prefix; the peer's keys stay outside the owner predicate,
        // and LIKE metacharacters and other text match literally.
        for prefix in [
            "ffffffff-ffff-7000-8000-000000000065",
            "event-00000000-0000-7000",
            "' OR true;",
            "%",
            "_",
            "event_",
            "event-%",
            "event-ffffffff-ffff-7000-8000-00000000006_",
            "\\",
        ] {
            assert!(
                TimePersistence::new(db.a.pool().clone())
                    .complete_time_values(&owner.identity, TimeCompletion::EventId, prefix, 101)
                    .await
                    .unwrap()
                    .is_empty(),
                "{prefix:?}"
            );
        }
        for limit in [0, 102] {
            assert!(
                TimePersistence::new(db.a.pool().clone())
                    .complete_time_values(&owner.identity, TimeCompletion::EventId, "", limit)
                    .await
                    .is_err()
            );
        }

        for version in [2, 12, 1] {
            catalog
                .create_epoch(
                    &owner,
                    Epoch {
                        epoch_id: EpochId::parse("epoch-00000000-0000-7000-8000-000000000001")
                            .unwrap(),
                        name: "launch".into(),
                        instant: instant(),
                        version: crate::TimeVersion::new(version).unwrap(),
                    },
                )
                .await
                .unwrap();
            catalog
                .create_calendar(
                    &owner,
                    crate::OperationalCalendarValue {
                        calendar_id: CalendarId::parse(
                            "calendar-00000000-0000-7000-8000-000000000001",
                        )
                        .unwrap(),
                        version: crate::TimeVersion::new(version).unwrap(),
                        name: "Operations".into(),
                        zone_id: "UTC".into(),
                        windows: Vec::new(),
                        excluded_dates: Vec::new(),
                    }
                    .build()
                    .unwrap(),
                )
                .await
                .unwrap();
        }
        catalog
            .create_calendar(
                &owner,
                crate::OperationalCalendarValue {
                    calendar_id: CalendarId::parse("calendar-00000000-0000-7000-8000-000000000002")
                        .unwrap(),
                    version: crate::TimeVersion::new(42).unwrap(),
                    name: "Other".into(),
                    zone_id: "UTC".into(),
                    windows: Vec::new(),
                    excluded_dates: Vec::new(),
                }
                .build()
                .unwrap(),
            )
            .await
            .unwrap();
        // A version published again: the same content answers it, other content conflicts.
        let again = crate::OperationalCalendarValue {
            calendar_id: CalendarId::parse("calendar-00000000-0000-7000-8000-000000000002")
                .unwrap(),
            version: crate::TimeVersion::new(42).unwrap(),
            name: "Other".into(),
            zone_id: "UTC".into(),
            windows: Vec::new(),
            excluded_dates: Vec::new(),
        };
        let replayed = catalog
            .create_calendar(&owner, again.clone().build().unwrap())
            .await
            .unwrap();
        assert_eq!(replayed, again.clone().build().unwrap());
        let renamed = crate::OperationalCalendarValue {
            name: "Renamed".into(),
            ..again
        };
        assert!(
            catalog
                .create_calendar(&owner, renamed.build().unwrap())
                .await
                .is_err()
        );
        let epoch = |name: &str| Epoch {
            epoch_id: EpochId::parse("epoch-00000000-0000-7000-8000-000000000001").unwrap(),
            name: name.into(),
            instant: instant(),
            version: crate::TimeVersion::new(12).unwrap(),
        };
        assert_eq!(
            catalog.create_epoch(&owner, epoch("launch")).await.unwrap(),
            epoch("launch")
        );
        assert!(
            catalog
                .create_epoch(&owner, epoch("relaunch"))
                .await
                .is_err()
        );
        let epoch_id = EpochId::parse("epoch-00000000-0000-7000-8000-000000000001").unwrap();
        assert_eq!(
            catalog
                .epoch(&owner, &epoch_id)
                .await
                .unwrap()
                .unwrap()
                .version
                .get(),
            12
        );
        assert_eq!(
            catalog
                .epoch(&peer, &epoch_id)
                .await
                .unwrap()
                .unwrap()
                .version
                .get(),
            12
        );
        assert!(catalog.epoch(&foreign, &epoch_id).await.unwrap().is_none());
        assert!(
            catalog
                .epoch(
                    &owner,
                    &EpochId::parse("epoch-00000000-0000-7000-8000-000000000002").unwrap()
                )
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(
                    &owner.identity,
                    TimeCompletion::EpochId,
                    "EPOCH-00000000-0000-7000-8000-000000000001",
                    101
                )
                .await
                .unwrap(),
            vec!["epoch-00000000-0000-7000-8000-000000000001"]
        );
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(
                    &owner.identity,
                    TimeCompletion::CalendarId,
                    "CALENDAR-00000000-0000-7000-8000-000000000001",
                    101
                )
                .await
                .unwrap(),
            vec!["calendar-00000000-0000-7000-8000-000000000001"]
        );
        let versions = || TimeCompletion::CalendarVersion {
            calendar_key: Some(
                CalendarId::parse("calendar-00000000-0000-7000-8000-000000000001").unwrap(),
            ),
        };
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(&peer.identity, versions(), "", 101)
                .await
                .unwrap(),
            vec!["1", "12", "2"]
        );
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(&peer.identity, versions(), "42", 101)
                .await
                .unwrap()
                .is_empty()
        );
        // A version prefix matches the leading digits only.
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(&peer.identity, versions(), "2", 101)
                .await
                .unwrap(),
            vec!["2"]
        );
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(&peer.identity, versions(), "1", 101)
                .await
                .unwrap(),
            vec!["1", "12"]
        );
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(&peer.identity, versions(), "%", 101)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(
                    &owner.identity,
                    TimeCompletion::EpochVersion {
                        epoch_key: Some(
                            EpochId::parse("epoch-00000000-0000-7000-8000-000000000001").unwrap()
                        ),
                    },
                    "1",
                    101
                )
                .await
                .unwrap(),
            vec!["1", "12"]
        );
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(&foreign.identity, versions(), "", 101)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(
                    &owner.identity,
                    TimeCompletion::CalendarVersion { calendar_key: None },
                    "42",
                    101
                )
                .await
                .unwrap(),
            vec!["42"]
        );

        // Late versions and state changes preserve strict page positions.
        for version in 13..=113 {
            catalog
                .create_epoch(
                    &owner,
                    Epoch {
                        epoch_id: epoch_id.clone(),
                        name: "launch".into(),
                        instant: instant(),
                        version: crate::TimeVersion::new(version).unwrap(),
                    },
                )
                .await
                .unwrap();
            catalog
                .create_calendar(
                    &owner,
                    crate::OperationalCalendarValue {
                        calendar_id: CalendarId::parse(
                            "calendar-00000000-0000-7000-8000-000000000001",
                        )
                        .unwrap(),
                        version: crate::TimeVersion::new(version).unwrap(),
                        name: "Operations".into(),
                        zone_id: "UTC".into(),
                        windows: Vec::new(),
                        excluded_dates: Vec::new(),
                    }
                    .build()
                    .unwrap(),
                )
                .await
                .unwrap();
        }
        let first = catalog
            .epochs_page(&owner, &selection::<crate::Epochs>())
            .await
            .unwrap();
        assert_eq!(first.items().len(), 100);
        assert_eq!(first.items()[0].version.get(), 113);
        assert!(first.next_cursor().is_some());
        let position: veoveo_types::CursorToken<crate::Epochs> =
            serde_json::from_value(serde_json::to_value(first.next_cursor().unwrap()).unwrap())
                .unwrap();
        let second = catalog
            .epochs_page(
                &owner,
                &selection::<crate::Epochs>().continuing(position.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            second
                .items()
                .iter()
                .map(|e| e.version.get())
                .collect::<Vec<_>>(),
            vec![13, 12, 2, 1]
        );
        assert!(second.next_cursor().is_none());
        assert!(
            catalog
                .epochs_page(
                    &foreign,
                    &selection::<crate::Epochs>().continuing(position.clone())
                )
                .await
                .is_err()
        );
        let first = catalog
            .calendars_page(&owner, &selection::<crate::Calendars>())
            .await
            .unwrap();
        assert_eq!(first.items().len(), 100);
        assert_eq!(first.items()[0].version.get(), 113);
        let position: veoveo_types::CursorToken<crate::Calendars> =
            serde_json::from_value(serde_json::to_value(first.next_cursor().unwrap()).unwrap())
                .unwrap();
        let second = catalog
            .calendars_page(
                &peer,
                &selection::<crate::Calendars>().continuing(position.clone()),
            )
            .await
            .unwrap();
        assert_eq!(second.items().len(), 5);
        assert!(second.next_cursor().is_none());
        assert_eq!(second.items().last().unwrap().version.get(), 42);
        let scheduled = catalog
            .events_page(&owner, &event_selection(Some(EventState::Scheduled)))
            .await
            .unwrap();
        assert_eq!(scheduled.items().len(), 100);
        assert!(scheduled.next_cursor().is_some());
        assert!(
            scheduled
                .items()
                .iter()
                .all(|e| e.state == EventState::Scheduled && e.event_id != own_id)
        );
        let position = scheduled.next_cursor().unwrap();
        let last_scheduled = catalog
            .events_page(
                &owner,
                &event_selection(Some(EventState::Scheduled)).continuing(position.clone()),
            )
            .await
            .unwrap();
        assert_eq!(last_scheduled.items().len(), 1);
        assert!(last_scheduled.next_cursor().is_none());

        // An unrelated malformed payload must never reach a request's epoch decoder.
        TimePersistence::new(db.a.pool().clone())
            .create_time_epoch(TimeEpochDraft {
                work_context: owner.work_context.clone(),
                identity: owner.identity.clone(),
                epoch_key: EpochId::parse("epoch-00000000-0000-7000-8000-000000000010").unwrap(),
                name: "unrelated".into(),
                epoch_version: crate::TimeVersion::new(1).unwrap(),
                tai_seconds_since_1970: 0,
                nanosecond: crate::SubsecondNanoseconds::ZERO,
                body: serde_json::json!({}),
            })
            .await
            .unwrap();
        let requested = vec![
            epoch_id.clone(),
            EpochId::parse("epoch-00000000-0000-7000-8000-000000000099").unwrap(),
        ];
        let epochs = catalog.epochs_for_keys(&owner, &requested).await.unwrap();
        assert_eq!(epochs.len(), 1);
        assert_eq!(epochs[0].version.get(), 113);
        assert!(
            catalog
                .epochs_for_keys(&foreign, &requested)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .latest_time_epochs(owner.identity.tenant_id, &vec![epoch_id.clone(); 101])
                .await
                .is_err()
        );
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .list_time_epochs(owner.identity.tenant_id, None, 102)
                .await
                .is_err()
        );
    })
    .await
    .expect("Time Store qualification exceeded 90 seconds");
}
