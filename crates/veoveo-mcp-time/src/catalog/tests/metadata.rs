use super::{instant, later, scope, selection};
use crate::{
    catalog::TimeCatalog,
    contract::*,
    test_database::{Row, TimeDb, body, rejected_by_schema, set_body, set_record_version},
};
use chrono::Utc;
use serde_json::json;
use std::time::Duration;
use veoveo_store::{PlatformStore, TenantId};

fn corrupt(
    original: &serde_json::Value,
    field: &str,
    value: serde_json::Value,
) -> serde_json::Value {
    let mut json = original.clone();
    json[field] = value;
    json
}

/// Set a stored subsecond column; the schema admits only `0..=999999999`.
async fn set_nanosecond(
    store: &PlatformStore,
    tenant: TenantId,
    table: &str,
    key: &str,
    value: i32,
) -> Result<i64, sqlx::Error> {
    sqlx::query_file_scalar!(
        "src/tests/queries/set_nanosecond.sql",
        table,
        tenant as TenantId,
        key,
        value,
    )
    .fetch_one(store.pool())
    .await
}

#[tokio::test]
async fn collection_reads_reject_identity_and_ordering_conflicts_after_sql_visibility() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "time-metadata", "owner").await;
        let peer = scope(&db.a, "time-metadata", "peer").await;
        let foreign = scope(&db.a, "time-metadata-other", "owner").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let calendar = catalog
            .create_calendar(
                &owner,
                crate::OperationalCalendarValue {
                    calendar_id: CalendarId::parse("calendar-00000000-0000-7000-8000-000000000001")
                        .unwrap(),
                    version: crate::TimeVersion::new(1).unwrap(),
                    name: "operations".into(),
                    zone_id: "UTC".into(),
                    windows: vec![],
                    excluded_dates: vec![],
                }
                .build()
                .unwrap(),
            )
            .await
            .unwrap();
        let row = Row::Calendar(&calendar.calendar_id);
        let original = body(&db.a, tenant, row).await;
        for (field, value) in [
            (
                "calendarId",
                json!("calendar-00000000-0000-7000-8000-000000000002"),
            ),
            ("version", json!(2)),
            ("name", json!("other")),
            ("zoneId", json!("Europe/London")),
        ] {
            let bad = corrupt(&original, field, value);
            serde_json::from_value::<crate::OperationalCalendar>(bad.clone())
                .expect("selected calendar corruption stays portable-admitted");
            set_body(&db.a, tenant, row, &bad).await;
            assert!(
                catalog
                    .calendar(&owner, &calendar.calendar_id, TimeVersion::new(1).unwrap())
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .calendars_page(&peer, &selection::<crate::Calendars>())
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .calendar(
                        &foreign,
                        &calendar.calendar_id,
                        TimeVersion::new(1).unwrap()
                    )
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                catalog
                    .calendars_page(&foreign, &selection::<crate::Calendars>())
                    .await
                    .unwrap()
                    .items()
                    .is_empty()
            );
            assert_eq!(
                body(&db.a, tenant, row).await,
                bad,
                "reads must leave retained data unchanged"
            );
        }
        set_body(&db.a, tenant, row, &original).await;
        assert_eq!(
            catalog
                .calendars_page(&owner, &selection::<crate::Calendars>())
                .await
                .unwrap()
                .items(),
            vec![calendar]
        );

        let epoch = catalog
            .create_epoch(
                &owner,
                Epoch {
                    epoch_id: EpochId::parse("epoch-00000000-0000-7000-8000-000000000001").unwrap(),
                    name: "mission".into(),
                    instant: instant(),
                    version: crate::TimeVersion::new(1).unwrap(),
                },
            )
            .await
            .unwrap();
        let row = Row::Epoch(&epoch.epoch_id);
        let original = body(&db.a, tenant, row).await;
        let mut wrong_instant = instant();
        wrong_instant.tai_seconds_since_1970 += 1;
        for (field, value) in [
            (
                "epochId",
                json!("epoch-00000000-0000-7000-8000-000000000002"),
            ),
            ("version", json!(2)),
            ("instant", json!(wrong_instant)),
        ] {
            let bad = corrupt(&original, field, value);
            set_body(&db.a, tenant, row, &bad).await;
            assert!(
                catalog.epoch(&owner, &epoch.epoch_id).await.is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .epochs_page(&owner, &selection::<crate::Epochs>())
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .epochs_for_keys(&peer, std::slice::from_ref(&epoch.epoch_id))
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .epochs_for_keys(&foreign, std::slice::from_ref(&epoch.epoch_id))
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(body(&db.a, tenant, row).await, bad);
        }
        set_body(&db.a, tenant, row, &original).await;
        assert_eq!(
            catalog
                .epochs_for_keys(&owner, std::slice::from_ref(&epoch.epoch_id))
                .await
                .unwrap(),
            vec![epoch]
        );

        let event = catalog
            .create_event(
                &owner,
                Event {
                    event_id: EventId::parse("event-00000000-0000-7000-8000-000000000001").unwrap(),
                    name: "launch".into(),
                    due: instant(),
                    state: EventState::Scheduled,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                later(),
                "launch".into(),
            )
            .await
            .unwrap();
        let row = Row::Event(&event.event_id);
        let original = body(&db.a, tenant, row).await;
        let mut wrong_due = instant();
        wrong_due.nanosecond =
            crate::SubsecondNanoseconds::new(wrong_due.nanosecond.get() + 1).unwrap();
        for (field, value) in [
            (
                "eventId",
                json!("event-00000000-0000-7000-8000-000000000002"),
            ),
            ("name", json!("other")),
            ("due", json!(wrong_due)),
        ] {
            let bad = corrupt(&original, field, value);
            set_body(&db.a, tenant, row, &bad).await;
            assert!(
                catalog.event(&owner, &event.event_id).await.is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .events_page(&owner, &selection::<crate::Events>())
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .cancel_event(&owner, &event.event_id, crate::TimeVersion::FIRST)
                    .await
                    .is_err(),
                "{field}"
            );
            for denied in [&peer, &foreign] {
                assert!(
                    catalog
                        .event(denied, &event.event_id)
                        .await
                        .unwrap()
                        .is_none()
                );
                assert!(
                    catalog
                        .events_page(denied, &selection::<crate::Events>())
                        .await
                        .unwrap()
                        .items()
                        .is_empty()
                );
            }
            assert_eq!(body(&db.a, tenant, row).await, bad);
        }
        let mut invalid_due = json!(instant());
        invalid_due["nanosecond"] = 1_000_000_000.into();
        set_body(&db.a, tenant, row, &corrupt(&original, "due", invalid_due)).await;
        assert!(
            catalog
                .event(&owner, &event.event_id)
                .await
                .unwrap_err()
                .to_string()
                .contains("body")
        );
        let error = set_nanosecond(
            &db.a,
            tenant,
            "event",
            event.event_id.as_str(),
            1_000_000_000,
        )
        .await
        .unwrap_err();
        assert!(rejected_by_schema(&error), "{error}");
        set_body(&db.a, tenant, row, &original).await;
        for version in [0_i64, -1] {
            let error = set_record_version(&db.a, tenant, row, version)
                .await
                .unwrap_err();
            assert!(rejected_by_schema(&error), "{error}");
        }
        assert_eq!(
            catalog
                .cancel_event(&owner, &event.event_id, crate::TimeVersion::FIRST)
                .await
                .unwrap()
                .state,
            EventState::Cancelled
        );
    })
    .await
    .expect("Time collection metadata qualification exceeded 90 seconds");
}

#[tokio::test]
async fn administrative_metadata_preserves_lifecycle_columns_and_rejects_conflicting_bodies() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "time-authority-metadata", "owner").await;
        let foreign = scope(&db.a, "time-authority-metadata-other", "owner").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let source = catalog
            .create_source(
                &owner,
                crate::NewTimeSourceValue {
                    source_id: TimeSourceId::parse(
                        "time-source-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    name: "IANA".into(),
                    dataset_kind: AuthorityDatasetKind::LeapSeconds,
                    url: "https://example.test/leaps".into(),
                    expected_content_type: "text/plain".into(),
                    enabled: true,
                    record_version: crate::SourceCreationVersion,
                }
                .build()
                .unwrap(),
            )
            .await
            .unwrap();
        let row = Row::Source(&source.source_id);
        let original = body(&db.a, tenant, row).await;
        for (field, value) in [
            (
                "sourceId",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("datasetKind", json!("tzdb")),
            ("url", json!("https://example.test/other")),
            ("expectedContentType", json!("application/json")),
            ("enabled", json!(false)),
            ("datasetKind", json!("STORED_SECRET_MARKER")),
        ] {
            let bad = corrupt(&original, field, value);
            set_body(&db.a, tenant, row, &bad).await;
            let error = catalog.source(&owner, &source.source_id).await.unwrap_err();
            assert!(!format!("{error:#}").contains("STORED_SECRET_MARKER"));
            assert!(
                catalog
                    .sources_page(&owner, &selection::<crate::Sources>())
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .source(&foreign, &source.source_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(body(&db.a, tenant, row).await, bad);
        }
        set_body(&db.a, tenant, row, &original).await;

        let now = Utc::now();
        let release = catalog
            .create_release(
                &owner,
                crate::AuthorityReleaseValue {
                    release_id: AuthorityReleaseId::parse(
                        "time-release-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    dataset_kind: source.dataset_kind,
                    version_label: "2026".into(),
                    source_url: source.url.clone(),
                    source_digest: veoveo_types::Sha256Digest::from_hex("a".repeat(64)).unwrap(),
                    artifact_path: "/var/lib/veoveo/time/releases/fixture/leaps".into(),
                    state: AuthorityReleaseState::Staged,
                    retrieved_at: now,
                    validated_at: now,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                }
                .build()
                .unwrap(),
            )
            .await
            .unwrap();
        catalog
            .activate_release(
                &owner,
                &release.release_id,
                crate::TimeVersion::FIRST,
                crate::TimeWriteGuard::new(0).unwrap(),
            )
            .await
            .unwrap();
        let row = Row::Release(&release.release_id);
        let active_body = body(&db.a, tenant, row).await;
        let mut second = crate::AuthorityReleaseValue::from(release.clone());
        second.release_id =
            AuthorityReleaseId::parse("time-release-00000000-0000-7000-8000-000000000002").unwrap();
        second.source_digest = veoveo_types::Sha256Digest::from_hex("b".repeat(64)).unwrap();
        let second = catalog
            .create_release(&owner, second.build().unwrap())
            .await
            .unwrap();
        catalog
            .activate_release(
                &owner,
                &second.release_id,
                crate::TimeVersion::FIRST,
                crate::TimeWriteGuard::new(1).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(body(&db.a, tenant, row).await, active_body);
        let retired = catalog
            .release(&owner, &release.release_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retired.state, AuthorityReleaseState::Retired);
        assert_eq!(retired.record_version.get(), 3);
        for (field, value) in [
            ("releaseId", json!(second.release_id)),
            (
                "sourceId",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("datasetKind", json!("tzdb")),
            ("sourceDigest", json!("c".repeat(64))),
            ("versionLabel", json!("other")),
            ("artifactPath", json!("/other")),
        ] {
            let bad = corrupt(&active_body, field, value);
            set_body(&db.a, tenant, row, &bad).await;
            assert!(
                catalog.release(&owner, &release.release_id).await.is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .releases_page(&owner, &selection::<crate::Releases>())
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .release(&foreign, &release.release_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(body(&db.a, tenant, row).await, bad);
        }
        set_body(&db.a, tenant, row, &active_body).await;

        let acquisition = catalog
            .create_acquisition(
                &owner,
                TimeAcquisition {
                    acquisition_id: TimeAcquisitionId::parse(
                        "time-acquisition-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    expected_source_digest: Some(
                        veoveo_types::Sha256Digest::from_hex("a".repeat(64)).unwrap(),
                    ),
                    status: TimeAcquisitionStatus::Succeeded,
                    phase: crate::TimeAcquisitionPhase::Complete,
                    staged_release_id: Some(release.release_id.clone()),
                    message: "".into(),
                    created_at: now,
                    updated_at: now,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                "acquire".into(),
            )
            .await
            .unwrap();
        let row = Row::Acquisition(&acquisition.acquisition_id);
        let original = body(&db.a, tenant, row).await;
        let digest = acquisition
            .expected_source_digest
            .as_ref()
            .map(|digest| digest.as_str().to_owned());
        // Unknown phases, staged releases that do not exist and malformed digests never
        // reach storage.
        for (phase, staged, expected) in [
            ("unknown", Some(release.release_id.as_str()), digest.clone()),
            ("staged", Some(release.release_id.as_str()), digest.clone()),
            ("", Some(release.release_id.as_str()), digest.clone()),
            ("complete", Some("time-release-bootstrap"), digest.clone()),
            (
                "complete",
                Some(release.release_id.as_str()),
                Some("PRIVATE_INVALID_DIGEST".to_owned()),
            ),
        ] {
            let error = sqlx::query_file_scalar!(
                "src/tests/queries/set_acquisition_columns.sql",
                tenant as TenantId,
                &acquisition.acquisition_id as &TimeAcquisitionId,
                phase,
                staged,
                expected,
            )
            .fetch_one(db.a.pool())
            .await
            .unwrap_err();
            assert!(rejected_by_schema(&error), "{phase}: {error}");
            assert_eq!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .unwrap()
                    .unwrap(),
                acquisition
            );
        }

        for (field, value) in [
            (
                "acquisitionId",
                json!("time-acquisition-00000000-0000-7000-8000-000000000002"),
            ),
            (
                "sourceId",
                json!("time-source-00000000-0000-7000-8000-000000000002"),
            ),
            ("expectedSourceDigest", json!("b".repeat(64))),
        ] {
            let bad = corrupt(&original, field, value);
            set_body(&db.a, tenant, row, &bad).await;
            assert!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .is_err(),
                "{field}"
            );
            assert!(
                catalog
                    .acquisition_for_idempotency(&owner, "acquire")
                    .await
                    .is_err()
            );
            assert!(
                catalog
                    .acquisition_for_release(&owner, &release.release_id)
                    .await
                    .is_err()
            );
            assert!(
                catalog
                    .acquisitions_page(&owner, &selection::<crate::Acquisitions>())
                    .await
                    .is_err()
            );
            assert!(
                catalog
                    .acquisition(&foreign, &acquisition.acquisition_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(body(&db.a, tenant, row).await, bad);
        }
        set_body(&db.a, tenant, row, &original).await;
        let mut wrong_source = acquisition.clone();
        wrong_source.source_id =
            TimeSourceId::parse("time-source-00000000-0000-7000-8000-000000000002").unwrap();
        let mut wrong_digest = acquisition.clone();
        wrong_digest.expected_source_digest = None;
        let mut wrong_creation = acquisition.clone();
        wrong_creation.created_at -= chrono::Duration::seconds(1);
        for wrong in [wrong_source, wrong_digest, wrong_creation] {
            veoveo_types::Check::check(&wrong).unwrap();
            assert!(catalog.update_acquisition(&owner, wrong).await.is_err());
            assert_eq!(
                body(&db.a, tenant, row).await,
                original,
                "invalid updates must not mutate storage"
            );
        }
        let mut updated = acquisition.clone();
        updated.status = TimeAcquisitionStatus::Succeeded;
        updated.phase = crate::TimeAcquisitionPhase::Complete;
        let updated = catalog.update_acquisition(&owner, updated).await.unwrap();
        assert_eq!(updated.record_version.get(), 2);
        let other = TimeCatalog::new(db.a.pool().clone());
        let mut left = updated.clone();
        left.message = "left concurrent update".into();
        let mut right = updated;
        right.message = "right concurrent update".into();
        let (left, right) = tokio::join!(
            catalog.update_acquisition(&owner, left),
            other.update_acquisition(&owner, right),
        );
        assert_ne!(left.is_ok(), right.is_ok(), "one version fence may advance");
        let winner = left.or(right).unwrap();
        assert_eq!(winner.record_version.get(), 3);
        assert!(matches!(
            winner.message.as_str(),
            "left concurrent update" | "right concurrent update"
        ));
        assert_eq!(
            catalog
                .acquisition(&owner, &acquisition.acquisition_id)
                .await
                .unwrap()
                .unwrap(),
            winner
        );
        // Mutable lifecycle columns supersede an earlier admitted queued body.
        let queued_body = corrupt(&original, "status", json!("queued"));
        let queued_body = corrupt(&queued_body, "phase", json!("queued"));
        let queued_body = corrupt(&queued_body, "stagedReleaseId", serde_json::Value::Null);
        let queued = serde_json::from_value::<TimeAcquisition>(queued_body.clone()).unwrap();
        set_body(&db.a, tenant, row, &queued_body).await;
        let restored = catalog
            .acquisition(&owner, &acquisition.acquisition_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.status, TimeAcquisitionStatus::Succeeded);
        assert_eq!(restored.phase, winner.phase);
        assert_eq!(restored.staged_release_id, winner.staged_release_id);
        assert_eq!(restored.updated_at, winner.updated_at);
        // Display metadata belongs to the body, unlike the lifecycle columns.
        assert_eq!(restored.message, queued.message);
        assert_eq!(restored.record_version.get(), 3);
        assert_eq!(
            body(&db.a, tenant, row).await,
            queued_body,
            "reading must not rewrite the stale body"
        );
    })
    .await
    .expect("Time administrative metadata qualification exceeded 90 seconds");
}

#[tokio::test]
async fn subsecond_columns_admit_one_second_and_corrupt_bodies_reject_reads() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "time-subseconds", "owner").await;
        let foreign = scope(&db.a, "time-subseconds-other", "owner").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let mut value = instant();
        value.nanosecond = SubsecondNanoseconds::MAX;
        let epoch = catalog
            .create_epoch(
                &owner,
                Epoch {
                    epoch_id: EpochId::parse("epoch-00000000-0000-7000-8000-000000000001").unwrap(),
                    name: "epoch".into(),
                    instant: value.clone(),
                    version: TimeVersion::FIRST,
                },
            )
            .await
            .unwrap();
        let event = catalog
            .create_event(
                &owner,
                Event {
                    event_id: EventId::parse("event-00000000-0000-7000-8000-000000000001").unwrap(),
                    name: "event".into(),
                    due: value,
                    state: EventState::Scheduled,
                    record_version: TimeVersion::FIRST,
                },
                later(),
                "subseconds".into(),
            )
            .await
            .unwrap();
        let rows = [
            (
                Row::Epoch(&epoch.epoch_id),
                "epoch",
                epoch.epoch_id.as_str(),
                "instant",
            ),
            (
                Row::Event(&event.event_id),
                "event",
                event.event_id.as_str(),
                "due",
            ),
        ];
        let mut originals = Vec::new();
        for (row, _, _, _) in &rows {
            originals.push(body(&db.a, tenant, *row).await);
        }
        for nanos in [-1_i32, 1_000_000_000, i32::MAX] {
            for (_, table, key, _) in &rows {
                let error = set_nanosecond(&db.a, tenant, table, key, nanos)
                    .await
                    .unwrap_err();
                assert!(rejected_by_schema(&error), "{table} {nanos}: {error}");
            }
        }
        for nanos in [-1_i64, 1_000_000_000, i64::from(u32::MAX), i64::MAX] {
            for ((row, _, _, field), original) in rows.iter().zip(&originals) {
                let mut corrupted = original.clone();
                corrupted[field]["nanosecond"] = nanos.into();
                set_body(&db.a, tenant, *row, &corrupted).await;
            }
            let errors = [
                catalog.epoch(&owner, &epoch.epoch_id).await.unwrap_err(),
                catalog
                    .epochs_page(&owner, &selection::<crate::Epochs>())
                    .await
                    .unwrap_err(),
                catalog.event(&owner, &event.event_id).await.unwrap_err(),
                catalog
                    .events_page(&owner, &selection::<crate::Events>())
                    .await
                    .unwrap_err(),
            ];
            for error in errors {
                let message = format!("{error:#}");
                assert!(message.contains("body"));
                assert!(!message.contains(&nanos.to_string()));
            }
            assert!(
                catalog
                    .epochs_page(&foreign, &selection::<crate::Epochs>())
                    .await
                    .unwrap()
                    .items()
                    .is_empty()
            );
            assert!(
                catalog
                    .events_page(&foreign, &selection::<crate::Events>())
                    .await
                    .unwrap()
                    .items()
                    .is_empty()
            );
            for ((row, _, _, field), original) in rows.iter().zip(&originals) {
                let mut expected = original.clone();
                expected[field]["nanosecond"] = nanos.into();
                assert_eq!(body(&db.a, tenant, *row).await, expected);
            }
        }
        for ((row, _, _, _), original) in rows.iter().zip(&originals) {
            set_body(&db.a, tenant, *row, original).await;
        }
        assert_eq!(
            catalog.epoch(&owner, &epoch.epoch_id).await.unwrap(),
            Some(epoch)
        );
        assert_eq!(
            catalog.event(&owner, &event.event_id).await.unwrap(),
            Some(event)
        );
    })
    .await
    .expect("Time subsecond metadata qualification exceeded 90 seconds");
}

#[tokio::test]
async fn retained_instants_reject_ambiguous_authority_bindings_after_sql_visibility() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "time-binding", "owner").await;
        let foreign = scope(&db.a, "time-binding-other", "owner").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let epoch = catalog
            .create_epoch(
                &owner,
                Epoch {
                    epoch_id: EpochId::parse("epoch-00000000-0000-7000-8000-000000000001").unwrap(),
                    name: "epoch".into(),
                    instant: instant(),
                    version: TimeVersion::FIRST,
                },
            )
            .await
            .unwrap();
        let event = super::event(
            &catalog,
            &owner,
            "event-00000000-0000-7000-8000-000000000001",
        )
        .await;
        let rows = [
            (Row::Epoch(&epoch.epoch_id), "instant"),
            (Row::Event(&event.event_id), "due"),
        ];
        let mut originals = Vec::new();
        let mut corrupted_bodies = Vec::new();
        for (row, field) in &rows {
            let original = body(&db.a, tenant, *row).await;
            let mut corrupted = original.clone();
            corrupted[field]["authority"]["leapSecondsReleaseId"] =
                corrupted[field]["authority"]["tzdbReleaseId"].clone();
            set_body(&db.a, tenant, *row, &corrupted).await;
            originals.push(original);
            corrupted_bodies.push(corrupted);
        }
        for error in [
            catalog.epoch(&owner, &epoch.epoch_id).await.unwrap_err(),
            catalog
                .epochs_page(&owner, &selection::<crate::Epochs>())
                .await
                .unwrap_err(),
            catalog.event(&owner, &event.event_id).await.unwrap_err(),
            catalog
                .events_page(&owner, &selection::<crate::Events>())
                .await
                .unwrap_err(),
        ] {
            assert!(error.to_string().contains("body"));
        }
        assert!(
            catalog
                .epoch(&foreign, &epoch.epoch_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            catalog
                .event(&foreign, &event.event_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            catalog
                .epochs_page(&foreign, &selection::<crate::Epochs>())
                .await
                .unwrap()
                .items()
                .is_empty()
        );
        assert!(
            catalog
                .events_page(&foreign, &selection::<crate::Events>())
                .await
                .unwrap()
                .items()
                .is_empty()
        );
        for (((row, _), original), corrupted) in rows.iter().zip(originals).zip(corrupted_bodies) {
            assert_eq!(body(&db.a, tenant, *row).await, corrupted);
            set_body(&db.a, tenant, *row, &original).await;
        }
        assert_eq!(
            catalog.epoch(&owner, &epoch.epoch_id).await.unwrap(),
            Some(epoch)
        );
        assert_eq!(
            catalog.event(&owner, &event.event_id).await.unwrap(),
            Some(event)
        );
    })
    .await
    .expect("Time authority binding metadata qualification exceeded 90 seconds");
}
