use super::*;
use crate::{
    catalog::{TimeAccessContext, TimeCatalog},
    contract::*,
    test_database::{Row, TimeDb, rejected_by_schema, set_body},
};
use std::time::Duration;
use uuid::Uuid;
use veoveo_store::{PlatformStore, PrincipalKind};

/// A named change that corrupts one stored row.
type Corruption<'a, Record> = (&'a str, Box<dyn Fn(&mut Record) + 'a>);

async fn scope(store: &PlatformStore, tenant: &str) -> TimeAccessContext {
    TimeAccessContext {
        work_context: "fixture-context".parse().unwrap(),
        identity: store
            .ensure_identity(
                tenant,
                "owner",
                "https://example.test",
                "owner",
                PrincipalKind::Service,
            )
            .await
            .unwrap(),
    }
}

async fn release(
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    kind: AuthorityDatasetKind,
) -> AuthorityRelease {
    let source = catalog
        .create_source(
            scope,
            crate::NewTimeSourceValue {
                source_id: TimeSourceId::parse(format!("time-source-{}", Uuid::now_v7())).unwrap(),
                name: "source".into(),
                dataset_kind: kind,
                url: "https://example.test/data".into(),
                expected_content_type: "text/plain".into(),
                enabled: true,
                record_version: crate::SourceCreationVersion,
            }
            .build()
            .unwrap(),
        )
        .await
        .unwrap();
    let now = Utc::now();
    catalog
        .create_release(
            scope,
            crate::AuthorityReleaseValue {
                release_id: AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7()))
                    .unwrap(),
                source_id: source.source_id.clone(),
                dataset_kind: kind,
                state: AuthorityReleaseState::Staged,
                version_label: "test".into(),
                source_url: source.url.clone(),
                source_digest: veoveo_types::Sha256Digest::from_hex(format!(
                    "{:064x}",
                    Uuid::now_v7().as_u128()
                ))
                .unwrap(),
                artifact_path: "/tmp/time-authority-test".into(),
                retrieved_at: now,
                validated_at: now,
                record_version: crate::TimeVersion::new(1).unwrap(),
            }
            .build()
            .unwrap(),
        )
        .await
        .unwrap()
}

async fn pointers(store: &PlatformStore, tenant: TenantId) -> Vec<TimeActiveAuthorityRecord> {
    sqlx::query_file_as!(
        TimeActiveAuthorityRecord,
        "src/persistence/queries/active_pointers.sql",
        tenant as TenantId,
    )
    .fetch_all(store.pool())
    .await
    .unwrap()
}

async fn raw_release(
    store: &PlatformStore,
    tenant: TenantId,
    key: &AuthorityReleaseId,
) -> TimeAuthorityReleaseRecord {
    sqlx::query_file_as!(
        TimeAuthorityReleaseRecord,
        "src/persistence/queries/release.sql",
        tenant as TenantId,
        key as &AuthorityReleaseId,
    )
    .fetch_one(store.pool())
    .await
    .unwrap()
}

/// Rewrite one pointer's columns from `pointer`, bypassing catalog admission.
async fn set_pointer(
    store: &PlatformStore,
    kind: TimeDatasetKind,
    pointer: &TimeActiveAuthorityRecord,
) -> Result<(), sqlx::Error> {
    sqlx::query_file_scalar!(
        "src/tests/queries/set_pointer.sql",
        pointer.tenant as TenantId,
        kind as TimeDatasetKind,
        pointer.tenant as TenantId,
        pointer.dataset_kind as TimeDatasetKind,
        &pointer.release_key as &AuthorityReleaseId,
        pointer.previous_release_key.as_ref() as Option<&AuthorityReleaseId>,
        pointer.record_version,
    )
    .fetch_one(store.pool())
    .await
    .map(drop)
}

/// Rewrite one release's relationship and lifecycle columns, bypassing admission.
async fn set_release(
    store: &PlatformStore,
    tenant: TenantId,
    key: &AuthorityReleaseId,
    release: &TimeAuthorityReleaseRecord,
) -> Result<(), sqlx::Error> {
    sqlx::query_file_scalar!(
        "src/tests/queries/set_release_columns.sql",
        tenant as TenantId,
        key as &AuthorityReleaseId,
        release.tenant as TenantId,
        release.dataset_kind as TimeDatasetKind,
        &release.release_key as &AuthorityReleaseId,
        release.state as TimeAuthorityReleaseState,
        release.source_digest,
        release.artifact_path,
    )
    .fetch_one(store.pool())
    .await
    .map(drop)
}

#[tokio::test]
async fn active_reads_reject_corrupt_pointers_and_filter_release_relationships_in_sql() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "active-parent").await;
        let foreign = scope(&db.a, "active-parent-other").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        assert!(catalog.active_releases(&owner).await.unwrap().is_empty());
        let current = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let candidate = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let other = release(&catalog, &foreign, AuthorityDatasetKind::Tzdb).await;
        for (scope, release) in [(&owner, &current), (&foreign, &other)] {
            catalog
                .activate_release(
                    scope,
                    &release.release_id,
                    TimeVersion::FIRST,
                    TimeWriteGuard::Absent,
                )
                .await
                .unwrap();
        }
        let selections = catalog.active_authorities(&owner).await.unwrap();
        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].pointer_version, TimeVersion::FIRST);
        assert_eq!(selections[0].release.record_version.get(), 2);
        assert_eq!(selections[0].release.release_id, current.release_id);
        assert_eq!(
            selections[0].write_guard(),
            TimeWriteGuard::Existing(TimeVersion::FIRST)
        );
        assert_eq!(
            catalog.active_authorities(&foreign).await.unwrap()[0]
                .release
                .release_id,
            other.release_id
        );
        let original = pointers(&db.a, tenant).await.remove(0);
        let original_release = raw_release(&db.a, tenant, &current.release_id).await;
        let original_candidate = raw_release(&db.a, tenant, &candidate.release_id).await;
        let unrelated =
            AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
        let invalid = AuthorityReleaseId::parse("time-release-private-invalid").unwrap();

        // The schema refuses a pointer whose version disagrees with its history, whose
        // release is missing or of another family or tenant, or that names itself.
        let corruptions: Vec<Corruption<'_, TimeActiveAuthorityRecord>> = vec![
            ("record_version 0", Box::new(|p| p.record_version = 0)),
            ("record_version -1", Box::new(|p| p.record_version = -1)),
            ("record_version 2", Box::new(|p| p.record_version = 2)),
            (
                "dataset_kind",
                Box::new(|p| p.dataset_kind = TimeDatasetKind::LeapSeconds),
            ),
            (
                "invalid release",
                Box::new(|p| p.release_key = invalid.clone()),
            ),
            (
                "foreign release",
                Box::new(|p| p.release_key = other.release_id.clone()),
            ),
            (
                "missing release",
                Box::new(|p| p.release_key = unrelated.clone()),
            ),
            (
                "self history",
                Box::new(|p| p.previous_release_key = Some(current.release_id.clone())),
            ),
            (
                "foreign history",
                Box::new(|p| p.previous_release_key = Some(other.release_id.clone())),
            ),
        ];
        for (case, corrupt) in corruptions {
            let mut bad = original.clone();
            corrupt(&mut bad);
            let error = set_pointer(&db.a, TimeDatasetKind::Tzdb, &bad)
                .await
                .unwrap_err();
            assert!(rejected_by_schema(&error), "{case}: {error}");
            assert_eq!(
                pointers(&db.a, tenant).await,
                std::slice::from_ref(&original),
                "{case}"
            );
        }

        // A staged release of the same family satisfies the schema; SQL selection and
        // admission reject the visible pointer.
        let mut staged = original.clone();
        staged.release_key = candidate.release_id.clone();
        set_pointer(&db.a, TimeDatasetKind::Tzdb, &staged)
            .await
            .unwrap();
        let error = catalog.active_releases(&owner).await.unwrap_err();
        assert!(error.to_string().contains("active_authority.release"));
        assert!(
            catalog
                .activate_release(
                    &owner,
                    &candidate.release_id,
                    TimeVersion::FIRST,
                    TimeWriteGuard::Existing(TimeVersion::FIRST)
                )
                .await
                .is_err()
        );
        assert_eq!(pointers(&db.a, tenant).await, [staged]);
        assert_eq!(
            raw_release(&db.a, tenant, &candidate.release_id).await,
            original_candidate
        );
        assert_eq!(
            catalog.active_releases(&foreign).await.unwrap()[0].release_id,
            other.release_id
        );
        set_pointer(&db.a, TimeDatasetKind::Tzdb, &original)
            .await
            .unwrap();

        // The schema refuses moving the selected release to another tenant, family or
        // key while a pointer references it.
        let release_corruptions: Vec<Corruption<'_, TimeAuthorityReleaseRecord>> = vec![
            (
                "tenant",
                Box::new(|r| r.tenant = foreign.identity.tenant_id),
            ),
            (
                "dataset_kind",
                Box::new(|r| r.dataset_kind = TimeDatasetKind::LeapSeconds),
            ),
            (
                "release_key",
                Box::new(|r| r.release_key = unrelated.clone()),
            ),
            (
                "source_digest",
                Box::new(|r| r.source_digest = "PRIVATE_INVALID_DIGEST".into()),
            ),
        ];
        for (case, corrupt) in release_corruptions {
            let mut bad = original_release.clone();
            corrupt(&mut bad);
            let error = set_release(&db.a, tenant, &current.release_id, &bad)
                .await
                .unwrap_err();
            assert!(rejected_by_schema(&error), "{case}: {error}");
            assert_eq!(
                raw_release(&db.a, tenant, &current.release_id).await,
                original_release,
                "{case}"
            );
        }
        for version in [0_i64, -1] {
            let error = crate::test_database::set_record_version(
                &db.a,
                tenant,
                Row::Release(&current.release_id),
                version,
            )
            .await
            .unwrap_err();
            assert!(rejected_by_schema(&error), "{error}");
        }

        // A release that is no longer active is absent from the SQL selection before
        // any body decoder runs. The visible pointer still fails.
        for state in [
            TimeAuthorityReleaseState::Staged,
            TimeAuthorityReleaseState::Retired,
            TimeAuthorityReleaseState::Quarantined,
        ] {
            let mut bad = original_release.clone();
            bad.state = state;
            set_release(&db.a, tenant, &current.release_id, &bad)
                .await
                .unwrap();
            set_body(
                &db.a,
                tenant,
                Row::Release(&current.release_id),
                &serde_json::json!({"marker": "private malformed body"}),
            )
            .await;
            let mut connection = db.b.pool().acquire().await.unwrap();
            let snapshot = active::read_snapshot(&mut connection, tenant)
                .await
                .unwrap();
            drop(connection);
            assert_eq!(snapshot.pointers.len(), 1);
            assert!(snapshot.releases.is_empty(), "{state:?}");
            let error = catalog.active_releases(&owner).await.unwrap_err();
            assert!(error.to_string().contains("active_authority.release"));
            assert!(!format!("{error:#}").contains("private malformed body"));
            assert!(
                catalog
                    .activate_release(
                        &owner,
                        &candidate.release_id,
                        TimeVersion::FIRST,
                        TimeWriteGuard::Existing(TimeVersion::FIRST)
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                raw_release(&db.a, tenant, &candidate.release_id).await,
                original_candidate
            );
            assert_eq!(
                pointers(&db.a, tenant).await,
                std::slice::from_ref(&original)
            );
            set_release(&db.a, tenant, &current.release_id, &original_release)
                .await
                .unwrap();
            set_body(
                &db.a,
                tenant,
                Row::Release(&current.release_id),
                &original_release.body,
            )
            .await;
        }
        assert_eq!(
            raw_release(&db.a, tenant, &current.release_id).await.body,
            original_release.body
        );

        let leaps = release(&catalog, &owner, AuthorityDatasetKind::LeapSeconds).await;
        catalog
            .activate_release(
                &owner,
                &leaps.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Absent,
            )
            .await
            .unwrap();
        let pair = catalog.active_releases(&owner).await.unwrap();
        assert_eq!(pair.len(), 2);
        assert_eq!(pair[0].release_id, leaps.release_id);
        assert_eq!(pair[1].release_id, current.release_id);
        // A selected release cannot disappear from under its pointer.
        let error = sqlx::query_file_scalar!(
            "src/tests/queries/delete_release.sql",
            tenant as TenantId,
            &current.release_id as &AuthorityReleaseId,
        )
        .fetch_optional(db.a.pool())
        .await
        .unwrap_err();
        assert!(rejected_by_schema(&error), "{error}");
        assert_eq!(catalog.active_releases(&owner).await.unwrap().len(), 2);
    })
    .await
    .expect("active authority parent qualification exceeded 90 seconds");
}

#[tokio::test]
async fn activation_rechecks_pointer_and_previous_release_relationships_inside_transaction() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let owner = scope(&db.a, "activation-parent").await;
        let tenant = owner.identity.tenant_id;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let first = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let second = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        let candidate = release(&catalog, &owner, AuthorityDatasetKind::Tzdb).await;
        catalog
            .activate_release(
                &owner,
                &first.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Absent,
            )
            .await
            .unwrap();
        catalog
            .activate_release(
                &owner,
                &second.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Existing(TimeVersion::FIRST),
            )
            .await
            .unwrap();
        let before_pointer = pointers(&db.a, tenant).await;
        let before_previous = raw_release(&db.a, tenant, &second.release_id).await;
        let before_candidate = raw_release(&db.a, tenant, &candidate.release_id).await;
        // Each fixture trigger changes a relationship after the candidate update, before
        // pointer replacement and retirement, inside the production transaction.
        for (index, case) in [
            "pointer_dataset_kind",
            "pointer_release_key",
            "pointer_previous_release_key",
            "pointer_record_version",
            "pointer_tenant",
            "release_state",
            "release_dataset_kind",
            "release_key",
            "release_record_version",
            "release_tenant",
        ]
        .into_iter()
        .enumerate()
        {
            interleave(&db.a, case).await;
            if index == 0 {
                sqlx::query_file!("src/tests/queries/interleave_trigger.sql")
                    .execute(db.a.pool())
                    .await
                    .unwrap();
            }
            assert!(
                catalog
                    .activate_release(
                        &owner,
                        &candidate.release_id,
                        TimeVersion::FIRST,
                        TimeWriteGuard::Existing(TimeVersion::new(2).unwrap())
                    )
                    .await
                    .is_err(),
                "{case}"
            );
            assert_eq!(pointers(&db.a, tenant).await, before_pointer, "{case}");
            assert_eq!(
                raw_release(&db.a, tenant, &second.release_id).await,
                before_previous,
                "{case}"
            );
            assert_eq!(
                raw_release(&db.a, tenant, &candidate.release_id).await,
                before_candidate,
                "{case}"
            );
        }
        sqlx::query_file!("src/tests/queries/interleave_drop.sql")
            .execute(db.a.pool())
            .await
            .unwrap();
        catalog
            .activate_release(
                &owner,
                &candidate.release_id,
                TimeVersion::FIRST,
                TimeWriteGuard::Existing(TimeVersion::new(2).unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(pointers(&db.a, tenant).await[0].record_version, 3);
        assert_eq!(
            raw_release(&db.a, tenant, &second.release_id).await.state,
            TimeAuthorityReleaseState::Retired
        );
        assert_eq!(
            catalog.active_releases(&owner).await.unwrap()[0].release_id,
            candidate.release_id
        );
    })
    .await
    .expect("transactional authority parent qualification exceeded 90 seconds");
}

/// Define the interleaving trigger function for one relationship change.
async fn interleave(store: &PlatformStore, case: &str) {
    let pool = store.pool();
    let defined = match case {
        "pointer_dataset_kind" => {
            sqlx::query_file!("src/tests/queries/interleave_pointer_dataset_kind.sql")
                .execute(pool)
                .await
        }
        "pointer_release_key" => {
            sqlx::query_file!("src/tests/queries/interleave_pointer_release_key.sql")
                .execute(pool)
                .await
        }
        "pointer_previous_release_key" => {
            sqlx::query_file!("src/tests/queries/interleave_pointer_previous_release_key.sql")
                .execute(pool)
                .await
        }
        "pointer_record_version" => {
            sqlx::query_file!("src/tests/queries/interleave_pointer_record_version.sql")
                .execute(pool)
                .await
        }
        "pointer_tenant" => {
            sqlx::query_file!("src/tests/queries/interleave_pointer_tenant.sql")
                .execute(pool)
                .await
        }
        "release_state" => {
            sqlx::query_file!("src/tests/queries/interleave_release_state.sql")
                .execute(pool)
                .await
        }
        "release_dataset_kind" => {
            sqlx::query_file!("src/tests/queries/interleave_release_dataset_kind.sql")
                .execute(pool)
                .await
        }
        "release_key" => {
            sqlx::query_file!("src/tests/queries/interleave_release_key.sql")
                .execute(pool)
                .await
        }
        "release_record_version" => {
            sqlx::query_file!("src/tests/queries/interleave_release_record_version.sql")
                .execute(pool)
                .await
        }
        "release_tenant" => {
            sqlx::query_file!("src/tests/queries/interleave_release_tenant.sql")
                .execute(pool)
                .await
        }
        other => panic!("unknown interleave case {other}"),
    };
    defined.unwrap();
}
