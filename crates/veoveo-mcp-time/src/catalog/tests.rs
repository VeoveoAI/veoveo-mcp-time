use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::catalog::TimeCompletion;
use crate::persistence::{TimeEpochDraft, TimeEventState as StoredEventState, TimePersistence};
use crate::{
    AuthorityBinding, AuthorityReleaseId, CalendarId, Epoch, EpochId, Event, EventId, EventState,
    OperationalCalendar, TimeInstant,
    catalog::{TimeAccessContext, TimeCatalog},
};
use veoveo_platform_store::{PlatformStore, PrincipalKind};

mod admin_pages;
mod knowledge;
mod metadata;
mod numeric;
mod precision;

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
        uncertainty_nanoseconds: 0,
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
        let page = catalog.events_page(&owner, None, None).await.unwrap();
        assert_eq!(page.limit, 100);
        assert_eq!(page.items.len(), 100);
        assert!(page.next_cursor.is_some());
        let last = page.items.last().unwrap();
        let cursor: crate::EventCursor = serde_json::from_value(
            serde_json::to_value(page.next_cursor.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(cursor.event_id(), &last.event_id);
        assert_eq!(cursor.tai_seconds(), last.due.tai_seconds_since_1970);
        assert_eq!(cursor.nanosecond(), last.due.nanosecond);
        let next = catalog
            .events_page(&owner, Some(&cursor), None)
            .await
            .unwrap();
        assert_eq!(next.items.len(), 2);
        assert!(next.next_cursor.is_none());
        assert_eq!(
            next.items[0].event_id.as_str(),
            "event-ffffffff-ffff-7000-8000-000000000064"
        );
        assert!(
            catalog
                .events_page(&foreign, Some(&cursor), None)
                .await
                .unwrap()
                .items
                .is_empty()
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
                    "FFFFFFFF-FFFF-7000-8000-000000000065",
                    101
                )
                .await
                .unwrap(),
            vec!["event-ffffffff-ffff-7000-8000-000000000065"]
        );
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(
                    &owner.identity,
                    TimeCompletion::EventId,
                    "00000000-0000-7000",
                    101
                )
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            TimePersistence::new(db.a.pool().clone())
                .complete_time_values(&owner.identity, TimeCompletion::EventId, "' OR true;", 101)
                .await
                .unwrap()
                .is_empty()
        );
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
        let first = catalog.epochs_page(&owner, None).await.unwrap();
        assert_eq!(first.items.len(), 100);
        assert_eq!(first.items[0].version.get(), 113);
        assert!(first.next_cursor.is_some());
        let last = first.items.last().unwrap();
        let position: crate::EpochCursor = serde_json::from_value(
            serde_json::to_value(first.next_cursor.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(position.epoch_id(), &last.epoch_id);
        assert_eq!(position.version(), last.version);
        let second = catalog.epochs_page(&owner, Some(&position)).await.unwrap();
        assert_eq!(
            second
                .items
                .iter()
                .map(|e| e.version.get())
                .collect::<Vec<_>>(),
            vec![13, 12, 2, 1]
        );
        assert!(second.next_cursor.is_none());
        assert!(
            catalog
                .epochs_page(&foreign, Some(&position))
                .await
                .unwrap()
                .items
                .is_empty()
        );
        let first = catalog.calendars_page(&owner, None).await.unwrap();
        assert_eq!(first.items.len(), 100);
        assert_eq!(first.items[0].version.get(), 113);
        let last = first.items.last().unwrap();
        let position: crate::CalendarCursor = serde_json::from_value(
            serde_json::to_value(first.next_cursor.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(position.calendar_id(), &last.calendar_id);
        assert_eq!(position.version(), last.version);
        let second = catalog
            .calendars_page(&peer, Some(&position))
            .await
            .unwrap();
        assert_eq!(second.items.len(), 5);
        assert!(second.next_cursor.is_none());
        assert_eq!(second.items.last().unwrap().version.get(), 42);
        let scheduled = catalog
            .events_page(&owner, None, Some(EventState::Scheduled))
            .await
            .unwrap();
        assert_eq!(scheduled.items.len(), 100);
        assert!(scheduled.next_cursor.is_some());
        assert!(
            scheduled
                .items
                .iter()
                .all(|e| e.state == EventState::Scheduled && e.event_id != own_id)
        );
        let last = scheduled.items.last().unwrap();
        let position = scheduled.next_cursor.as_ref().unwrap();
        assert_eq!(position.event_id(), &last.event_id);
        let last_scheduled = catalog
            .events_page(&owner, Some(position), Some(EventState::Scheduled))
            .await
            .unwrap();
        assert_eq!(last_scheduled.items.len(), 1);
        assert!(last_scheduled.next_cursor.is_none());

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
