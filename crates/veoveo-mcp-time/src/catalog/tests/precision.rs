//! Records built from nanosecond timestamps keep microsecond precision, so their JSON
//! bodies and PostgreSQL's microsecond columns read back as the same instants.
use super::{instant, scope};
use crate::{
    catalog::TimeCatalog,
    contract::*,
    persistence::{
        PersistenceError, TimeAuthorityReleaseDraft, TimeAuthorityReleaseState, TimeDatasetKind,
        TimeEventDraft, TimeEventState, TimePersistence,
    },
    test_database::{Row, TimeDb, body},
};
use chrono::{TimeDelta, TimeZone, Utc};
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn nanosecond_timestamps_round_trip_through_the_database_at_microsecond_precision() {
    let db = TimeDb::new().await;
    tokio::time::timeout(Duration::from_secs(90), async {
        let owner = scope(&db.a, "time-precision", "owner").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let precise = Utc.timestamp_opt(2_000_000_000, 123_456_789).unwrap();
        let recorded = Utc.timestamp_opt(2_000_000_000, 123_456_000).unwrap();
        let recorded_wire = serde_json::to_value(recorded).unwrap();

        let source = catalog
            .create_source(
                &owner,
                crate::NewTimeSourceValue {
                    source_id: TimeSourceId::parse(format!("time-source-{}", Uuid::now_v7()))
                        .unwrap(),
                    name: "precision".into(),
                    dataset_kind: AuthorityDatasetKind::LeapSeconds,
                    url: "https://example.test/leap-seconds.list".into(),
                    expected_content_type: "text/plain".into(),
                    enabled: true,
                    record_version: crate::SourceCreationVersion,
                }
                .build()
                .unwrap(),
            )
            .await
            .unwrap();

        // The builder records both release timestamps at microsecond precision; the
        // column checks compare them with the body on every read.
        let release = crate::AuthorityReleaseValue {
            release_id: AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7()))
                .unwrap(),
            source_id: source.source_id.clone(),
            dataset_kind: AuthorityDatasetKind::LeapSeconds,
            version_label: "precision".into(),
            source_url: source.url.clone(),
            source_digest: veoveo_types::Sha256Digest::from_hex("d".repeat(64)).unwrap(),
            artifact_path: "/var/lib/veoveo/time/releases/precision/leap-seconds.list".into(),
            state: AuthorityReleaseState::Staged,
            retrieved_at: precise,
            validated_at: precise + TimeDelta::nanoseconds(1),
            record_version: TimeVersion::FIRST,
        }
        .build()
        .unwrap();
        assert_eq!(release.retrieved_at, recorded);
        assert_eq!(release.validated_at, recorded);
        assert_eq!(
            catalog
                .create_release(&owner, release.clone())
                .await
                .unwrap(),
            release
        );
        assert_eq!(
            catalog
                .release(&owner, &release.release_id)
                .await
                .unwrap()
                .unwrap(),
            release
        );
        assert_eq!(
            catalog.releases_page(&owner, None).await.unwrap().items,
            std::slice::from_ref(&release)
        );
        let stored = body(&db.a, tenant, Row::Release(&release.release_id)).await;
        assert_eq!(stored["retrievedAt"], recorded_wire);
        assert_eq!(stored["validatedAt"], recorded_wire);
        catalog
            .activate_release(
                &owner,
                &release.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::new(0).unwrap(),
            )
            .await
            .unwrap();
        let active = catalog.active_releases(&owner).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].retrieved_at, recorded);
        assert_eq!(active[0].validated_at, recorded);

        // The catalog records acquisition timestamps at microsecond precision, and reads
        // take both lifecycle times from the database clock's columns.
        let acquisition = catalog
            .create_acquisition(
                &owner,
                TimeAcquisition {
                    acquisition_id: TimeAcquisitionId::parse(format!(
                        "time-acquisition-{}",
                        Uuid::now_v7()
                    ))
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    expected_source_digest: Some(release.source_digest.clone()),
                    status: TimeAcquisitionStatus::Succeeded,
                    phase: TimeAcquisitionPhase::Complete,
                    staged_release_id: Some(release.release_id.clone()),
                    message: "".into(),
                    created_at: precise,
                    updated_at: precise,
                    record_version: TimeVersion::FIRST,
                },
                "precision".into(),
            )
            .await
            .unwrap();
        let stored = body(&db.a, tenant, Row::Acquisition(&acquisition.acquisition_id)).await;
        assert_eq!(stored["createdAt"], recorded_wire);
        assert_eq!(stored["updatedAt"], recorded_wire);
        let read = catalog
            .acquisition(&owner, &acquisition.acquisition_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read, acquisition);
        let mut progress = read.clone();
        progress.message = "recorded".into();
        let updated = catalog.update_acquisition(&owner, progress).await.unwrap();
        assert_eq!(updated.created_at, read.created_at);
        assert!(updated.updated_at >= updated.created_at);
        assert_eq!(
            catalog
                .acquisition(&owner, &acquisition.acquisition_id)
                .await
                .unwrap()
                .unwrap(),
            updated
        );

        // An event's due instant rounds up, so the due loop never fires it early.
        let event_id = EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap();
        let event = Event {
            event_id: event_id.clone(),
            name: "precision".into(),
            due: instant(),
            state: EventState::Scheduled,
            record_version: TimeVersion::FIRST,
        };
        catalog
            .create_event(&owner, event.clone(), precise, "precision".into())
            .await
            .unwrap();
        let persistence = TimePersistence::new(db.a.pool().clone());
        let stored = persistence
            .time_event(&owner.identity, &event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.due,
            Utc.timestamp_opt(2_000_000_000, 123_457_000).unwrap()
        );
        assert_eq!(catalog.event(&owner, &event_id).await.unwrap(), Some(event));

        // A writer that bypasses the catalog cannot bind a sub-microsecond timestamp.
        let rejected = persistence
            .create_time_authority_release(TimeAuthorityReleaseDraft {
                work_context: owner.work_context.clone(),
                identity: owner.identity.clone(),
                release_key: AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7()))
                    .unwrap(),
                source_key: source.source_id.clone(),
                dataset_kind: TimeDatasetKind::LeapSeconds,
                state: TimeAuthorityReleaseState::Staged,
                version_label: "precision".into(),
                source_url: source.url.clone(),
                source_digest: veoveo_types::Sha256Digest::from_hex("e".repeat(64)).unwrap(),
                artifact_path: "/var/lib/veoveo/time/releases/precision/other".into(),
                retrieved_at: precise,
                validated_at: precise,
                body: serde_json::json!({}),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            rejected,
            PersistenceError::InvalidTimeField {
                field: "retrieved_at",
                ..
            }
        ));
        let rejected = persistence
            .create_time_event(TimeEventDraft {
                work_context: owner.work_context.clone(),
                identity: owner.identity.clone(),
                event_key: EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap(),
                name: "precision".into(),
                state: TimeEventState::Scheduled,
                due_tai_seconds_since_1970: instant().tai_seconds_since_1970,
                due_nanosecond: instant().nanosecond,
                due: precise,
                idempotency_key: "bypass".into(),
                body: serde_json::json!({}),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            rejected,
            PersistenceError::InvalidTimeField { field: "due", .. }
        ));
    })
    .await
    .unwrap();
}
