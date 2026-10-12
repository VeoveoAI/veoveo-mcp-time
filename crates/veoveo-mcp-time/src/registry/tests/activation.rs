use super::*;
use crate::persistence::{
    TimeActiveAuthorityRecord, TimeAuthorityReleaseRecord, TimeAuthorityReleaseState,
    TimeDatasetKind, TimePersistence,
};
use std::time::Duration;
use veoveo_platform_store::{PrincipalId, TenantId};

async fn prepare(
    registry: &AuthorityRegistry,
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    release: &AuthorityRelease,
    pointer: TimeWriteGuard,
) -> ActivationDraft {
    registry
        .preflight_activation(
            catalog,
            scope,
            &release.release_id,
            TimeVersion::FIRST,
            pointer,
        )
        .await
        .unwrap()
}

async fn raw_release(
    store: &PlatformStore,
    tenant: TenantId,
    key: &AuthorityReleaseId,
) -> TimeAuthorityReleaseRecord {
    TimePersistence::new(store.pool().clone())
        .time_authority_release(tenant, key)
        .await
        .unwrap()
        .unwrap()
}

/// Rewrite a release's artifact path, bypassing the catalog.
async fn set_artifact_path(
    store: &PlatformStore,
    release: &TimeAuthorityReleaseRecord,
    path: &str,
) {
    sqlx::query_file_scalar!(
        "src/tests/queries/set_release_columns.sql",
        release.tenant as TenantId,
        &release.release_key as &AuthorityReleaseId,
        release.tenant as TenantId,
        release.dataset_kind as TimeDatasetKind,
        &release.release_key as &AuthorityReleaseId,
        release.state as TimeAuthorityReleaseState,
        release.source_digest,
        path,
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn activation_serializes_both_families_from_the_preflighted_pair() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let left = TimeCatalog::new(db.a.pool().clone());
        let right = TimeCatalog::new(db.b.pool().clone());
        let registry = files.registry();
        let replica = files.registry();
        let owner = scope(&db.a, "pair-activation").await;
        for generation in 0..4 {
            let (tzdb, _) = stage(&left, &owner, AuthorityDatasetKind::Tzdb, &files.tzdb).await;
            let (leaps, _) = stage(
                &right,
                &owner,
                AuthorityDatasetKind::LeapSeconds,
                &files.first_leaps,
            )
            .await;
            let guard = TimeWriteGuard::new(generation).unwrap();
            let tzdb_draft = prepare(&registry, &left, &owner, &tzdb, guard).await;
            let leap_draft = prepare(&replica, &right, &owner, &leaps, guard).await;
            let (tzdb_result, leap_result) = tokio::join!(
                left.commit_activation(&owner, tzdb_draft),
                right.commit_activation(&owner, leap_draft),
            );
            assert_ne!(
                tzdb_result.is_ok(),
                leap_result.is_ok(),
                "only one decision from the same pair may commit: {tzdb_result:?}, {leap_result:?}"
            );
            let (winner, loser) = if tzdb_result.is_ok() {
                (&tzdb, &leaps)
            } else {
                (&leaps, &tzdb)
            };
            let unchanged = left
                .release(&owner, &loser.release_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(unchanged.state, AuthorityReleaseState::Staged);
            assert_eq!(unchanged.record_version.get(), 1);
            let committed = left
                .release(&owner, &winner.release_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(committed.state, AuthorityReleaseState::Active);
            // A caller must load the changed pair again before retrying.
            activate(&right, &replica, &owner, loser, guard).await;
            let pair = left.active_releases(&owner).await.unwrap();
            assert_eq!(pair.len(), 2);
            assert!(pair.iter().any(|r| r.release_id == tzdb.release_id));
            assert!(pair.iter().any(|r| r.release_id == leaps.release_id));
        }
    })
    .await
    .expect("cross-family activation qualification exceeded 90 seconds");
}

#[tokio::test]
async fn changed_preflight_inputs_reject_publication_without_advancing_authority() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let registry = files.registry();
        let owner = scope(&db.a, "changed-preflight").await;
        let foreign = scope(&db.a, "foreign-preflight").await;
        let tenant = owner.identity.tenant_id;
        let (leaps, _) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        activate(&catalog, &registry, &owner, &leaps, TimeWriteGuard::Absent).await;
        let (tzdb, _) = stage(&catalog, &owner, AuthorityDatasetKind::Tzdb, &files.tzdb).await;
        for target in [&leaps, &tzdb] {
            let draft = prepare(&registry, &catalog, &owner, &tzdb, TimeWriteGuard::Absent).await;
            let record = raw_release(&db.a, tenant, &target.release_id).await;
            set_artifact_path(&db.a, &record, "/changed-after-preflight").await;
            assert!(catalog.commit_activation(&owner, draft).await.is_err());
            set_artifact_path(&db.a, &record, &target.artifact_path).await;
            let pair = catalog.active_releases(&owner).await.unwrap();
            assert_eq!(pair.len(), 1);
            assert_eq!(pair[0].release_id, leaps.release_id);
            let candidate = catalog
                .release(&owner, &tzdb.release_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(candidate.state, AuthorityReleaseState::Staged);
            assert_eq!(candidate.record_version.get(), 1);
        }
        let draft = prepare(&registry, &catalog, &owner, &tzdb, TimeWriteGuard::Absent).await;
        assert!(catalog.commit_activation(&foreign, draft).await.is_err());
        assert!(catalog.active_releases(&foreign).await.unwrap().is_empty());

        let draft = prepare(&registry, &catalog, &owner, &tzdb, TimeWriteGuard::Absent).await;
        sqlx::query_file_scalar!("src/tests/queries/delete_pointers.sql", tenant as TenantId,)
            .fetch_all(db.a.pool())
            .await
            .unwrap();
        assert!(
            catalog.commit_activation(&owner, draft).await.is_err(),
            "deleted other-family pointer"
        );
    })
    .await
    .expect("changed preflight qualification exceeded 90 seconds");
}

#[tokio::test]
async fn failed_file_preflight_never_publishes_an_authority() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let registry = files.registry();
        let owner = scope(&db.a, "failed-file-preflight").await;
        let (leaps, _) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        tokio::fs::remove_file(&files.first_leaps).await.unwrap();
        assert!(
            registry
                .activate_release(
                    &catalog,
                    &owner,
                    &leaps.release_id,
                    TimeVersion::FIRST,
                    TimeWriteGuard::Absent
                )
                .await
                .is_err()
        );
        assert!(catalog.active_releases(&owner).await.unwrap().is_empty());
        assert_eq!(
            catalog
                .release(&owner, &leaps.release_id)
                .await
                .unwrap()
                .unwrap()
                .state,
            AuthorityReleaseState::Staged
        );
    })
    .await
    .expect("failed file preflight qualification exceeded 90 seconds");
}

/// Whether `error` is PostgreSQL's `lock_not_available`.
fn lock_not_available(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.code().as_deref() == Some("55P03"))
}

#[tokio::test]
async fn activation_locks_hold_off_concurrent_activations_and_input_repairs() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.a.pool().clone());
        let registry = files.registry();
        let owner = scope(&db.a, "locked-inputs").await;
        let tenant = owner.identity.tenant_id;
        let mut releases = Vec::new();
        for (kind, path) in [
            (AuthorityDatasetKind::LeapSeconds, &files.first_leaps),
            (AuthorityDatasetKind::Tzdb, &files.tzdb),
        ] {
            let (release, _) = stage(&catalog, &owner, kind, path).await;
            activate(
                &catalog,
                &registry,
                &owner,
                &release,
                TimeWriteGuard::Absent,
            )
            .await;
            releases.push(release.release_id.clone());
        }
        // An activation holds the tenant lock and its locked rows until it ends.
        let mut activation = db.a.pool().begin().await.unwrap();
        sqlx::query_file_scalar!(
            "src/persistence/queries/lock_tenant_activation.sql",
            tenant as TenantId,
        )
        .fetch_one(&mut *activation)
        .await
        .unwrap();
        let pointers = sqlx::query_file_as!(
            TimeActiveAuthorityRecord,
            "src/persistence/queries/lock_pointers.sql",
            tenant as TenantId,
        )
        .fetch_all(&mut *activation)
        .await
        .unwrap();
        assert_eq!(pointers.len(), 2);
        sqlx::query_file_scalar!(
            "src/persistence/queries/lock_releases.sql",
            tenant as TenantId,
            &releases as &[AuthorityReleaseId],
        )
        .fetch_all(&mut *activation)
        .await
        .unwrap();

        // Another activation of the tenant waits, even for a family without a pointer.
        let mut writer = db.b.pool().begin().await.unwrap();
        sqlx::query_file_scalar!("src/tests/queries/lock_timeout.sql", "200ms")
            .fetch_one(&mut *writer)
            .await
            .unwrap();
        let blocked = sqlx::query_file_scalar!(
            "src/persistence/queries/lock_tenant_activation.sql",
            tenant as TenantId,
        )
        .fetch_one(&mut *writer)
        .await
        .unwrap_err();
        assert!(lock_not_available(&blocked), "{blocked}");
        writer.rollback().await.unwrap();

        // A writer that repairs a locked pointer or release waits for the activation.
        for pointer in &pointers {
            let mut writer = db.b.pool().begin().await.unwrap();
            sqlx::query_file_scalar!("src/tests/queries/lock_timeout.sql", "200ms")
                .fetch_one(&mut *writer)
                .await
                .unwrap();
            let blocked = sqlx::query_file_scalar!(
                "src/tests/queries/set_pointer.sql",
                tenant as TenantId,
                pointer.dataset_kind as TimeDatasetKind,
                tenant as TenantId,
                pointer.dataset_kind as TimeDatasetKind,
                &pointer.release_key as &AuthorityReleaseId,
                pointer.previous_release_key.as_ref() as Option<&AuthorityReleaseId>,
                pointer.record_version,
            )
            .fetch_one(&mut *writer)
            .await
            .unwrap_err();
            assert!(lock_not_available(&blocked), "{blocked}");
            writer.rollback().await.unwrap();
        }
        for release in &releases {
            let mut writer = db.b.pool().begin().await.unwrap();
            sqlx::query_file_scalar!("src/tests/queries/lock_timeout.sql", "200ms")
                .fetch_one(&mut *writer)
                .await
                .unwrap();
            let blocked = sqlx::query_file_scalar!(
                "src/tests/queries/set_record_version.sql",
                "authority_release",
                tenant as TenantId,
                release.as_str(),
                2_i64,
            )
            .fetch_one(&mut *writer)
            .await
            .unwrap_err();
            assert!(lock_not_available(&blocked), "{blocked}");
            writer.rollback().await.unwrap();
        }
        activation.rollback().await.unwrap();
        // Once the activation ends, the same repair proceeds.
        crate::test_database::set_record_version(&db.b, tenant, Row::Release(&releases[0]), 2)
            .await
            .unwrap();
    })
    .await
    .expect("locked activation inputs exceeded 90 seconds");
}
