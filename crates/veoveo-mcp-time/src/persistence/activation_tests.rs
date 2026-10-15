use super::{activation::ActivationConflict, *};
use crate::test_database::{Row, TimeDb, set_record_version};
use uuid::Uuid;
use veoveo_store::PrincipalKind;

#[tokio::test]
async fn time_authority_activation_retires_the_previous_release_atomically() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let identity =
            db.a.ensure_identity(
                "tenant-time",
                "time-admin",
                "https://veoveo.local/services",
                "time-admin",
                PrincipalKind::Service,
            )
            .await
            .unwrap();
        let store = TimePersistence::new(db.b.pool().clone());
        let source_key = TimeSourceId::parse(format!("time-source-{}", Uuid::now_v7())).unwrap();
        store
            .create_time_source(TimeSourceDraft {
                identity: identity.clone(),
                source_key: source_key.clone(),
                name: "IANA leap seconds".to_owned(),
                dataset_kind: TimeDatasetKind::LeapSeconds,
                source_url: "https://example.com/leap-seconds.list".to_owned(),
                expected_content_type: "text/plain".to_owned(),
                enabled: true,
                body: serde_json::json!({"sourceId": source_key}),
            })
            .await
            .unwrap();
        let create_release =
            |release_key: AuthorityReleaseId, digest: String| TimeAuthorityReleaseDraft {
                work_context: "fixture-context".parse().unwrap(),
                identity: identity.clone(),
                release_key,
                source_key: source_key.clone(),
                dataset_kind: TimeDatasetKind::LeapSeconds,
                state: TimeAuthorityReleaseState::Staged,
                version_label: format!("iana-{}", &digest[..12]),
                source_url: "https://example.com/leap-seconds.list".to_owned(),
                source_digest: veoveo_types::Sha256Digest::from_hex(&digest).unwrap(),
                artifact_path: "/var/lib/veoveo/time/releases/test/leap-seconds.list".to_owned(),
                retrieved_at: crate::contract::admission::stored_timestamp(Utc::now()),
                validated_at: crate::contract::admission::stored_timestamp(Utc::now()),
                body: serde_json::json!({"state": "staged"}),
            };
        let active = || serde_json::json!({"state": "active"});

        let first_key =
            AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
        store
            .create_time_authority_release(create_release(first_key.clone(), "a".repeat(64)))
            .await
            .unwrap();
        let first = store
            .activate_time_authority_release(
                &identity,
                &first_key,
                crate::TimeVersion::new(1).unwrap(),
                crate::TimeWriteGuard::new(0).unwrap(),
                active(),
            )
            .await
            .unwrap();
        assert_eq!(first.state, TimeAuthorityReleaseState::Active);

        let second_key =
            AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
        store
            .create_time_authority_release(create_release(second_key.clone(), "b".repeat(64)))
            .await
            .unwrap();
        let second = store
            .activate_time_authority_release(
                &identity,
                &second_key,
                crate::TimeVersion::new(1).unwrap(),
                crate::TimeWriteGuard::new(1).unwrap(),
                active(),
            )
            .await
            .unwrap();
        assert_eq!(second.state, TimeAuthorityReleaseState::Active);
        let retired = store
            .time_authority_release(identity.tenant_id, &first_key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retired.state, TimeAuthorityReleaseState::Retired);
        assert_eq!(retired.record_version, 3);
        let pointer = store
            .active_time_authority(identity.tenant_id, TimeDatasetKind::LeapSeconds)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pointer.release_key, second_key);
        assert_eq!(pointer.previous_release_key.as_ref(), Some(&first_key));
        assert_eq!(pointer.record_version.get(), 2);
        let left_key =
            AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
        let right_key =
            AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
        for (key, digest) in [(&left_key, "c"), (&right_key, "d")] {
            store
                .create_time_authority_release(create_release(key.clone(), digest.repeat(64)))
                .await
                .unwrap();
        }
        let other = TimePersistence::new(db.a.pool().clone());
        let (left, right) = tokio::join!(
            store.activate_time_authority_release(
                &identity,
                &left_key,
                crate::TimeVersion::new(1).unwrap(),
                crate::TimeWriteGuard::new(2).unwrap(),
                active(),
            ),
            other.activate_time_authority_release(
                &identity,
                &right_key,
                crate::TimeVersion::new(1).unwrap(),
                crate::TimeWriteGuard::new(2).unwrap(),
                active(),
            ),
        );
        assert_ne!(
            left.is_ok(),
            right.is_ok(),
            "only one replacement may advance the pointer"
        );
        let losing = left.as_ref().err().or(right.as_ref().err()).unwrap();
        assert!(
            matches!(
                losing,
                PersistenceError::ActivationConflict {
                    conflict: ActivationConflict::Pair | ActivationConflict::Pointer,
                    ..
                }
            ),
            "{losing}"
        );
        let (winner, loser) = if left.is_ok() {
            (&left_key, &right_key)
        } else {
            (&right_key, &left_key)
        };
        let pointer = other
            .active_time_authority(identity.tenant_id, TimeDatasetKind::LeapSeconds)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&pointer.release_key, winner);
        assert_eq!(pointer.record_version.get(), 3);
        let loser = other
            .time_authority_release(identity.tenant_id, loser)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loser.state, TimeAuthorityReleaseState::Staged);
        assert_eq!(loser.record_version, 1);
        assert_eq!(
            other
                .time_authority_release(identity.tenant_id, &second_key)
                .await
                .unwrap()
                .unwrap()
                .state,
            TimeAuthorityReleaseState::Retired
        );

        // Exhaustion in the previous release must roll back the new release and
        // pointer together; neither may commit before retirement succeeds.
        set_record_version(&db.a, identity.tenant_id, Row::Release(winner), i64::MAX)
            .await
            .unwrap();
        let candidate =
            AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
        store
            .create_time_authority_release(create_release(candidate.clone(), "e".repeat(64)))
            .await
            .unwrap();
        let exhausted = store
            .activate_time_authority_release(
                &identity,
                &candidate,
                TimeVersion::FIRST,
                TimeWriteGuard::Existing(TimeVersion::new(3).unwrap()),
                active(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                exhausted,
                PersistenceError::ActivationConflict {
                    conflict: ActivationConflict::PreviousRelease,
                    ..
                }
            ),
            "{exhausted}"
        );
        let unchanged = store
            .time_authority_release(identity.tenant_id, &candidate)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.state, TimeAuthorityReleaseState::Staged);
        assert_eq!(unchanged.record_version, 1);
        let unchanged = store
            .active_time_authority(identity.tenant_id, TimeDatasetKind::LeapSeconds)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&unchanged.release_key, winner);
        assert_eq!(unchanged.record_version.get(), 3);
        let unchanged = store
            .time_authority_release(identity.tenant_id, winner)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.state, TimeAuthorityReleaseState::Active);
        assert_eq!(unchanged.record_version, i64::MAX);

        set_record_version(&db.a, identity.tenant_id, Row::Release(winner), 2)
            .await
            .unwrap();
        store
            .activate_time_authority_release(
                &identity,
                &candidate,
                TimeVersion::FIRST,
                TimeWriteGuard::Existing(TimeVersion::new(3).unwrap()),
                active(),
            )
            .await
            .unwrap();
        let pointer = store
            .active_time_authority(identity.tenant_id, TimeDatasetKind::LeapSeconds)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pointer.release_key, candidate);
        assert_eq!(pointer.record_version.get(), 4);
        let retired = store
            .time_authority_release(identity.tenant_id, winner)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retired.state, TimeAuthorityReleaseState::Retired);
        assert_eq!(retired.record_version, 3);
    })
    .await
    .expect("authority activation qualification exceeded 90 seconds");
}
