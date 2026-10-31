mod activation;
#[cfg(feature = "mcp")]
mod active_authorities;
#[cfg(feature = "mcp")]
mod app;
mod digest;
mod due;
#[cfg(feature = "mcp")]
mod events;
#[cfg(feature = "mcp")]
mod hosted;
mod lifecycle;
mod notifications;
#[cfg(feature = "mcp")]
mod permissions;
#[cfg(feature = "mcp")]
mod tool_input_tests;
use super::*;
use crate::contract::*;
use crate::test_database::{Row, TimeDb, body, rejected_by_schema, set_body};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use veoveo_store::{PlatformStore, PrincipalKind};
use veoveo_types::Sha256Digest;

/// Copied UTC tzdata and leap files that Time's registries and replicas load.
pub(crate) struct AuthorityFiles {
    /// Owns the files' directory, which dropping the fixture removes.
    _root: tempfile::TempDir,
    tzdb: PathBuf,
    bootstrap_leaps: PathBuf,
    first_leaps: PathBuf,
    next_leaps: PathBuf,
    bootstrap: AuthorityContext,
}

impl AuthorityFiles {
    pub(crate) async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let tzdb = root.path().join("tzdb");
        tokio::fs::create_dir_all(tzdb.join("Mission"))
            .await
            .unwrap();
        // Same tzdata prerequisite as the existing temporal-engine tests.
        tokio::fs::copy("/usr/share/zoneinfo/UTC", tzdb.join("UTC"))
            .await
            .unwrap();
        tokio::fs::copy("/usr/share/zoneinfo/UTC", tzdb.join("Mission/Test"))
            .await
            .unwrap();
        let bootstrap_leaps = root.path().join("bootstrap.list");
        let first_leaps = root.path().join("first.list");
        let next_leaps = root.path().join("next.list");
        for (path, offset) in [
            (&bootstrap_leaps, 10),
            (&first_leaps, 11),
            (&next_leaps, 12),
        ] {
            tokio::fs::write(path, format!("2272060800 {offset}\n"))
                .await
                .unwrap();
        }
        let bootstrap = AuthorityContext::from_paths(
            EffectiveTimeAuthority::new(
                bootstrap_reference("time-release-bootstrap-tzdb", AuthorityDatasetKind::Tzdb),
                bootstrap_reference(
                    "time-release-bootstrap-leaps",
                    AuthorityDatasetKind::LeapSeconds,
                ),
            )
            .unwrap(),
            &tzdb,
            LeapSecondTable::from_path(&bootstrap_leaps).await.unwrap(),
        )
        .unwrap();
        Self {
            _root: root,
            tzdb,
            bootstrap_leaps,
            first_leaps,
            next_leaps,
            bootstrap,
        }
    }

    fn registry(&self) -> AuthorityRegistry {
        AuthorityRegistry::new(
            self.bootstrap.clone(),
            self.tzdb.clone(),
            self.bootstrap_leaps.clone(),
        )
    }

    /// One Time replica on `store` with its own process instance and authority cache, as
    /// one process runs it. It starts no change watcher, due loop or Task consumer.
    #[cfg(feature = "mcp")]
    pub(crate) async fn replica(
        &self,
        store: &PlatformStore,
    ) -> Arc<crate::state::TimeApplication> {
        // The acquisition client uses rustls with ring, as the server installs it.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let catalog = TimeCatalog::new(store.pool().clone());
        let acquisitions = crate::acquisition::AcquisitionService::new(
            crate::acquisition::AcquisitionServiceConfig {
                scratch_root: self.tzdb.with_file_name("scratch"),
                release_root: self.tzdb.with_file_name("releases"),
                zic_executable: self.tzdb.with_file_name("unavailable-zic"),
                maximum_source_bytes: 1024,
                maximum_expanded_bytes: 4096,
                timeout: Duration::from_secs(1),
            },
            catalog.clone(),
        )
        .unwrap();
        let slug = crate::test_database::slug();
        let instance = veoveo_store::Instance::start(store, &slug).await.unwrap();
        Arc::new(crate::state::TimeApplication {
            store: store.clone(),
            tasks: veoveo_task::TaskRuntime::new(
                store.clone(),
                slug,
                veoveo_types::DeploymentId::parse("019a5b30-0000-7000-8000-000000000001").unwrap(),
                Arc::new(instance),
            ),
            catalog,
            authorities: self.registry(),
            clock: crate::clock::ClockMonitor::new(
                crate::clock::ClockSource::System,
                Duration::from_secs(1),
            ),
            acquisitions: Arc::new(acquisitions),
            subscriptions: Arc::new(crate::changes::TenantSubscriptions::default()),
        })
    }
}

fn bootstrap_reference(id: &str, kind: AuthorityDatasetKind) -> TimeAuthorityReference {
    let release_id = AuthorityReleaseId::parse(id).unwrap();
    TimeAuthorityReference::new(
        TimeAuthorityReleaseUri::bootstrap(&release_id),
        kind,
        TimeAuthoritySource::Bootstrap {},
        Sha256Digest::from_hex("a".repeat(64)).unwrap(),
        "fixture".into(),
    )
    .unwrap()
}

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

async fn stage(
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    kind: AuthorityDatasetKind,
    path: &std::path::Path,
) -> (AuthorityRelease, TimeAcquisition) {
    let source = catalog
        .create_source(
            scope,
            crate::NewTimeSourceValue {
                source_id: TimeSourceId::parse(format!("time-source-{}", Uuid::now_v7())).unwrap(),
                name: "fixture".into(),
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
    let bytes = match kind {
        AuthorityDatasetKind::Tzdb => tokio::fs::read(path.join("UTC")).await.unwrap(),
        AuthorityDatasetKind::LeapSeconds => tokio::fs::read(path).await.unwrap(),
    };
    let release = catalog
        .create_release(
            scope,
            crate::AuthorityReleaseValue {
                release_id: AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7()))
                    .unwrap(),
                source_id: source.source_id.clone(),
                dataset_kind: kind,
                state: AuthorityReleaseState::Staged,
                version_label: "fixture".into(),
                source_url: source.url.clone(),
                source_digest: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
                artifact_path: path.to_str().unwrap().into(),
                retrieved_at: now,
                validated_at: now,
                record_version: crate::TimeVersion::new(1).unwrap(),
            }
            .build()
            .unwrap(),
        )
        .await
        .unwrap();
    let acquisition = catalog
        .create_acquisition(
            scope,
            TimeAcquisition {
                acquisition_id: TimeAcquisitionId::parse(format!(
                    "time-acquisition-{}",
                    Uuid::now_v7()
                ))
                .unwrap(),
                source_id: source.source_id.clone(),
                expected_source_digest: Some(release.source_digest.clone()),
                status: TimeAcquisitionStatus::Succeeded,
                phase: crate::TimeAcquisitionPhase::Complete,
                staged_release_id: Some(release.release_id.clone()),
                message: "".into(),
                created_at: now,
                updated_at: now,
                record_version: crate::TimeVersion::new(1).unwrap(),
            },
            Uuid::now_v7().to_string(),
        )
        .await
        .unwrap();
    (release, acquisition)
}

async fn activate(
    catalog: &TimeCatalog,
    registry: &AuthorityRegistry,
    scope: &TimeAccessContext,
    release: &AuthorityRelease,
    expected: TimeWriteGuard,
) {
    registry
        .activate_release(
            catalog,
            scope,
            &release.release_id,
            TimeVersion::FIRST,
            expected,
        )
        .await
        .unwrap();
}

fn resolved(engine: &TemporalEngine) -> TimeInstant {
    engine
        .resolve(&ResolveTimeRequest {
            expression: crate::TimeExpressionValue::Rfc3339 {
                value: "2024-01-01T00:00:00Z".into(),
            }
            .build()
            .unwrap(),
            additional_uncertainty: crate::NanosecondBound::ZERO,
        })
        .unwrap()
        .into_instant()
}

/// Set an acquisition's staged release, bypassing the catalog.
async fn set_staged_release(
    store: &PlatformStore,
    scope: &TimeAccessContext,
    acquisition: &TimeAcquisition,
    staged: Option<&AuthorityReleaseId>,
) {
    sqlx::query_file_scalar!(
        "src/tests/queries/set_acquisition_columns.sql",
        scope.identity.tenant_id as veoveo_store::TenantId,
        &acquisition.acquisition_id as &TimeAcquisitionId,
        acquisition.phase as crate::TimeAcquisitionPhase,
        staged.map(AuthorityReleaseId::as_str),
        acquisition
            .expected_source_digest
            .as_ref()
            .map(Sha256Digest::as_str),
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn replicas_load_persisted_authority_before_serving_and_validate_cache_reuse() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let registry = files.registry();
        let writer = TimeCatalog::new(db.a.pool().clone());
        let reader = TimeCatalog::new(db.b.pool().clone());
        let owner = scope(&db.a, "registry-owner").await;
        let foreign = scope(&db.a, "registry-other").await;
        let tenant = owner.identity.tenant_id;
        let bootstrap = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            bootstrap.authority().effective(),
            files.bootstrap.effective()
        );
        let (first, acquisition) = stage(
            &writer,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        activate(&writer, &registry, &owner, &first, TimeWriteGuard::Absent).await;
        // No notification or manual reload: request-time selection is authoritative.
        let first_engine = registry.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            first_engine.authority().binding().leap_seconds_release_id(),
            &first.release_id
        );
        assert_eq!(
            resolved(&first_engine).tai_seconds_since_1970,
            resolved(&bootstrap).tai_seconds_since_1970 + 1
        );
        let reused = registry.authority_engine(&reader, &owner).await.unwrap();
        assert!(Arc::ptr_eq(
            first_engine.authority().leap_seconds(),
            reused.authority().leap_seconds()
        ));
        assert_eq!(
            registry
                .authority_engine(&reader, &foreign)
                .await
                .unwrap()
                .authority()
                .effective(),
            files.bootstrap.effective()
        );

        let (tzdb, _) = stage(&writer, &owner, AuthorityDatasetKind::Tzdb, &files.tzdb).await;
        activate(&writer, &registry, &owner, &tzdb, TimeWriteGuard::Absent).await;
        let restarted = files.registry();
        let engine = restarted.authority_engine(&reader, &owner).await.unwrap();
        assert_eq!(
            engine.authority().binding().tzdb_release_id(),
            &tzdb.release_id
        );
        assert_eq!(
            engine.authority().binding().leap_seconds_release_id(),
            &first.release_id
        );
        assert!(
            engine
                .authority()
                .tzdb()
                .available()
                .any(|zone| zone.to_string() == "Mission/Test")
        );
        let epoch_id = EpochId::parse("epoch-private").unwrap();
        engine.replace_epochs([Epoch {
            epoch_id: epoch_id.clone(),
            name: "request local".into(),
            version: crate::TimeVersion::new(1).unwrap(),
            instant: resolved(&engine),
        }]);
        let isolated = restarted.authority_engine(&reader, &owner).await.unwrap();
        assert!(
            isolated
                .resolve(&ResolveTimeRequest {
                    expression: crate::TimeExpressionValue::EpochRelative {
                        epoch_id,
                        offset: crate::TimeSpan::ZERO
                    }
                    .build()
                    .unwrap(),
                    additional_uncertainty: crate::NanosecondBound::ZERO
                })
                .is_err()
        );

        // A warm context cannot hide missing or malformed producing provenance.
        set_staged_release(&db.a, &owner, &acquisition, None).await;
        assert!(registry.authority_engine(&reader, &owner).await.is_err());
        set_staged_release(&db.a, &owner, &acquisition, Some(&first.release_id)).await;
        assert!(registry.authority_engine(&reader, &owner).await.is_ok());
        let release_row = Row::Release(&first.release_id);
        let release_body = body(&db.a, tenant, release_row).await;
        set_body(&db.a, tenant, release_row, &serde_json::json!({})).await;
        assert!(registry.authority_engine(&reader, &owner).await.is_err());
        assert_eq!(
            registry
                .bootstrap_reference(files.bootstrap.binding().tzdb_release_id())
                .unwrap(),
            files.bootstrap.effective().tzdb().clone()
        );
        assert!(registry.bootstrap_reference(&first.release_id).is_none());
        set_body(&db.a, tenant, release_row, &release_body).await;
        assert!(registry.authority_engine(&reader, &owner).await.is_ok());

        // Explicit reload failure evicts the old context; repair can recover it.
        tokio::fs::remove_file(&files.first_leaps).await.unwrap();
        assert!(registry.reload(&reader, &owner).await.is_err());
        assert!(registry.authority_engine(&reader, &owner).await.is_err());
        tokio::fs::write(&files.first_leaps, "2272060800 11\n")
            .await
            .unwrap();
        assert!(registry.authority_engine(&reader, &owner).await.is_ok());
        // The active release cannot disappear from under its pointer.
        let error = sqlx::query_file_scalar!(
            "src/tests/queries/delete_release.sql",
            tenant as veoveo_store::TenantId,
            &first.release_id as &AuthorityReleaseId,
        )
        .fetch_optional(db.a.pool())
        .await
        .unwrap_err();
        assert!(rejected_by_schema(&error), "{error}");
        assert!(registry.authority_engine(&reader, &owner).await.is_ok());
        assert_eq!(
            registry
                .authority_engine(&reader, &foreign)
                .await
                .unwrap()
                .authority()
                .effective(),
            files.bootstrap.effective()
        );
    })
    .await
    .expect("authority registry qualification exceeded 90 seconds");
}
