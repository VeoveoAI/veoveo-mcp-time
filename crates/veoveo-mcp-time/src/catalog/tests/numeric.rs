use super::{instant, later, scope};
use crate::{
    catalog::TimeCatalog,
    contract::*,
    persistence::{TimeClockPolicyRecord, TimePersistence},
    test_database::{Row, TimeDb, rejected_by_schema, set_record_version},
};
use std::time::Duration;
use veoveo_platform_store::{PlatformStore, TenantId};

fn policy(stratum: u8) -> ClockQualityPolicy {
    ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(1000)
        .maximum_stratum(stratum)
        .minimum_source_diversity(2)
        .maximum_holdover_seconds(300)
        .build()
        .unwrap()
}

async fn clock_row(store: &PlatformStore, tenant: TenantId) -> TimeClockPolicyRecord {
    TimePersistence::new(store.pool().clone())
        .time_clock_policy(tenant)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn clock_policy_rows_admit_only_policy_ranges_and_positive_versions() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "time-clock-numeric", "owner").await;
        let foreign = scope(&db.a, "time-clock-numeric-other", "owner").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let (created, version) = catalog
            .replace_clock_policy(&owner, policy(4), TimeWriteGuard::Absent)
            .await
            .unwrap();
        assert_eq!(created, policy(4));
        assert_eq!(version, TimeVersion::FIRST);
        assert!(
            catalog
                .replace_clock_policy(&owner, policy(3), TimeWriteGuard::Absent)
                .await
                .is_err()
        );
        let retained = clock_row(&db.a, tenant).await;
        for (error_bound, stratum, diversity, holdover) in [
            (-1_i64, 4_i16, 2_i64, 300_i64),
            (0, 4, 2, 300),
            (1000, -1, 2, 300),
            (1000, 0, 2, 300),
            (1000, 16, 2, 300),
            (1000, 256, 2, 300),
            (1000, 4, -1, 300),
            (1000, 4, 0, 300),
            (1000, 4, i64::from(u32::MAX) + 1, 300),
            (1000, 4, 2, -1),
            (1000, 4, 2, 0),
        ] {
            let error = sqlx::query_file_scalar!(
                "src/tests/queries/set_clock_policy_limits.sql",
                tenant as TenantId,
                error_bound,
                stratum,
                diversity,
                holdover,
            )
            .fetch_one(db.a.pool())
            .await
            .unwrap_err();
            assert!(rejected_by_schema(&error), "{error}");
            assert_eq!(clock_row(&db.a, tenant).await, retained);
        }
        for version in [-1, 0] {
            let error = set_record_version(&db.a, tenant, Row::ClockPolicy, version)
                .await
                .unwrap_err();
            assert!(rejected_by_schema(&error), "{error}");
        }
        assert_eq!(
            catalog.clock_policy(&owner).await.unwrap().unwrap(),
            (policy(4), TimeVersion::FIRST)
        );
        assert!(catalog.clock_policy(&foreign).await.unwrap().is_none());
        let other = TimeCatalog::new(db.a.pool().clone());
        let (left, right) = tokio::join!(
            catalog.replace_clock_policy(
                &owner,
                policy(3),
                TimeWriteGuard::Existing(TimeVersion::FIRST)
            ),
            other.replace_clock_policy(
                &owner,
                policy(5),
                TimeWriteGuard::Existing(TimeVersion::FIRST)
            ),
        );
        assert_ne!(left.is_ok(), right.is_ok());
        let winner = left.or(right).unwrap();
        assert_eq!(winner.1.get(), 2);
        assert_eq!(catalog.clock_policy(&owner).await.unwrap().unwrap(), winner);
        set_record_version(&db.a, tenant, Row::ClockPolicy, i64::MAX)
            .await
            .unwrap();
        let retained = clock_row(&db.a, tenant).await;
        let (_, max) = catalog.clock_policy(&owner).await.unwrap().unwrap();
        assert_eq!(max.get(), i64::MAX as u64);
        assert!(
            catalog
                .replace_clock_policy(&owner, policy(6), TimeWriteGuard::Existing(max))
                .await
                .is_err()
        );
        assert_eq!(clock_row(&db.a, tenant).await, retained);
    })
    .await
    .expect("Time clock numeric qualification exceeded 90 seconds");
}

#[tokio::test]
async fn exhausted_source_acquisition_and_event_versions_cannot_advance() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "time-version-numeric", "owner").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let max = TimeVersion::new(i64::MAX as u64).unwrap();
        let source = catalog
            .create_source(
                &owner,
                crate::NewTimeSourceValue {
                    source_id: TimeSourceId::parse(
                        "time-source-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    name: "source".into(),
                    dataset_kind: AuthorityDatasetKind::Tzdb,
                    url: "https://example.test/tzdb".into(),
                    expected_content_type: "application/gzip".into(),
                    enabled: true,
                    record_version: crate::SourceCreationVersion,
                }
                .build()
                .unwrap(),
            )
            .await
            .unwrap();
        set_record_version(&db.a, tenant, Row::Source(&source.source_id), i64::MAX)
            .await
            .unwrap();
        let retained = catalog
            .source(&owner, &source.source_id)
            .await
            .unwrap()
            .unwrap();
        let mut changed = crate::TimeSourceValue::from(retained.clone());
        changed.enabled = false;
        assert!(
            catalog
                .replace_source(&owner, changed.build().unwrap(), max)
                .await
                .is_err()
        );
        assert_eq!(
            catalog
                .source(&owner, &source.source_id)
                .await
                .unwrap()
                .unwrap(),
            retained
        );

        let now = chrono::Utc::now();
        let acquisition = catalog
            .create_acquisition(
                &owner,
                TimeAcquisition {
                    acquisition_id: TimeAcquisitionId::parse(
                        "time-acquisition-00000000-0000-7000-8000-000000000001",
                    )
                    .unwrap(),
                    source_id: source.source_id.clone(),
                    expected_source_digest: None,
                    status: TimeAcquisitionStatus::Queued,
                    phase: crate::TimeAcquisitionPhase::Queued,
                    staged_release_id: None,
                    message: "".into(),
                    created_at: now,
                    updated_at: now,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                "numeric".into(),
            )
            .await
            .unwrap();
        set_record_version(
            &db.a,
            tenant,
            Row::Acquisition(&acquisition.acquisition_id),
            i64::MAX,
        )
        .await
        .unwrap();
        let retained = catalog
            .acquisition(&owner, &acquisition.acquisition_id)
            .await
            .unwrap()
            .unwrap();
        for version in [0, i64::MAX as u64, u64::MAX] {
            let mut wire = serde_json::to_value(&retained).unwrap();
            wire["recordVersion"] = version.into();
            wire["status"] = "running".into();
            wire["phase"] = "downloading".into();
            match serde_json::from_value::<TimeAcquisition>(wire) {
                Ok(changed) => {
                    assert_eq!(version, i64::MAX as u64);
                    assert!(catalog.update_acquisition(&owner, changed).await.is_err());
                }
                Err(error) => assert!(
                    crate::TimeVersion::new(version).is_err(),
                    "an admitted exhausted version must reach the update fence: {error}"
                ),
            }
            assert_eq!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .unwrap()
                    .unwrap(),
                retained
            );
        }
        let event = catalog
            .create_event(
                &owner,
                Event {
                    event_id: EventId::parse("event-00000000-0000-7000-8000-000000000001").unwrap(),
                    name: "event".into(),
                    due: instant(),
                    state: EventState::Scheduled,
                    record_version: crate::TimeVersion::new(1).unwrap(),
                },
                later(),
                "numeric".into(),
            )
            .await
            .unwrap();
        set_record_version(&db.a, tenant, Row::Event(&event.event_id), i64::MAX)
            .await
            .unwrap();
        let retained = catalog
            .event(&owner, &event.event_id)
            .await
            .unwrap()
            .unwrap();
        assert!(
            catalog
                .cancel_event(&owner, &event.event_id, max)
                .await
                .is_err()
        );
        // The due statement skips an exhausted event instead of failing the batch.
        set_due_now(&db.a, tenant, &event.event_id).await;
        assert!(catalog.mark_due_events().await.unwrap().is_empty());
        assert_eq!(
            catalog
                .event(&owner, &event.event_id)
                .await
                .unwrap()
                .unwrap(),
            retained
        );
    })
    .await
    .expect("Time version numeric qualification exceeded 90 seconds");
}

/// Make a stored event due a second ago, bypassing the catalog.
pub(crate) async fn set_due_now(store: &PlatformStore, tenant: TenantId, event: &EventId) {
    sqlx::query_file_scalar!(
        "src/tests/queries/set_event_due.sql",
        tenant as TenantId,
        event as &EventId,
        chrono::Utc::now() - chrono::TimeDelta::seconds(1),
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
}
