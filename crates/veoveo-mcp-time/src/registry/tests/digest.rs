use super::*;

#[tokio::test]
async fn retained_digests_carry_provenance_idempotency_and_reject_corruption() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.clone());
        let owner = scope(&db.a, "digest-owner").await;
        let foreign = scope(&db.a, "digest-other").await;
        let (release, mut acquisition) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        assert!(release.source_digest.as_str().starts_with("sha256:"));
        let record = RecordId::new("time_authority_release", release.release_id.to_string());
        let retained = catalog
            .release(&owner, &release.release_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained, release);
        assert_eq!(
            catalog
                .authority_reference(&owner, &retained)
                .await
                .unwrap()
                .source_digest(),
            &release.source_digest
        );
        let registry = files.registry();
        activate(
            &catalog,
            &registry,
            &owner,
            &retained,
            TimeWriteGuard::Absent,
        )
        .await;
        assert_eq!(
            registry
                .authority_engine(&catalog, &owner)
                .await
                .unwrap()
                .authority()
                .effective()
                .leap_seconds()
                .source_digest(),
            &release.source_digest
        );

        acquisition.acquisition_id =
            TimeAcquisitionId::parse(format!("time-acquisition-{}", Uuid::now_v7())).unwrap();
        acquisition.expected_source_digest = Some(release.source_digest.clone());
        acquisition.staged_release_id = None;
        acquisition.status = TimeAcquisitionStatus::Queued;
        acquisition.phase = crate::TimeAcquisitionPhase::Queued;
        let created = catalog
            .create_acquisition(&owner, acquisition.clone(), "digest-replay".into())
            .await
            .unwrap();
        acquisition.acquisition_id =
            TimeAcquisitionId::parse(format!("time-acquisition-{}", Uuid::now_v7())).unwrap();
        let replay = catalog
            .create_acquisition(&owner, acquisition.clone(), "digest-replay".into())
            .await
            .unwrap();
        assert_eq!(replay.acquisition_id, created.acquisition_id);
        assert_eq!(
            replay.expected_source_digest,
            created.expected_source_digest
        );
        acquisition.expected_source_digest = None;
        assert!(
            catalog
                .create_acquisition(&owner, acquisition, "digest-replay".into())
                .await
                .is_err(),
            "an idempotency key replays only the request that created it"
        );

        // Corrupt both representations identically: equality alone must not admit
        // malformed hashes. Errors must neither echo the digest nor leak the row.
        let mut body = serde_json::to_value(&retained).unwrap();
        body["sourceDigest"] = "PRIVATE_INVALID_DIGEST".into();
        set(
            &db.a,
            record.clone(),
            "source_digest",
            "PRIVATE_INVALID_DIGEST",
        )
        .await;
        set(&db.a, record, "canonical_json", body.to_string()).await;
        let error = catalog
            .release(&owner, &release.release_id)
            .await
            .unwrap_err();
        assert!(!format!("{error:#}").contains("PRIVATE_INVALID_DIGEST"));
        assert!(catalog.list_releases(&owner).await.is_err());
        assert!(
            catalog
                .release(&foreign, &release.release_id)
                .await
                .unwrap()
                .is_none()
        );
        let mut body = serde_json::to_value(&created).unwrap();
        body["expectedSourceDigest"] = "PRIVATE_INVALID_DIGEST".into();
        let record = RecordId::new("time_acquisition", created.acquisition_id.to_string());
        set(
            &db.a,
            record.clone(),
            "expected_source_digest",
            "PRIVATE_INVALID_DIGEST",
        )
        .await;
        set(&db.a, record, "canonical_json", body.to_string()).await;
        let error = catalog
            .acquisition(&owner, &created.acquisition_id)
            .await
            .unwrap_err();
        assert!(!format!("{error:#}").contains("PRIVATE_INVALID_DIGEST"));
        assert!(
            catalog
                .acquisition_for_idempotency(&owner, "digest-replay")
                .await
                .is_err()
        );
        assert!(
            catalog
                .acquisition(&foreign, &created.acquisition_id)
                .await
                .unwrap()
                .is_none()
        );
    })
    .await
    .expect("retained digest qualification exceeded 90 seconds");
}
