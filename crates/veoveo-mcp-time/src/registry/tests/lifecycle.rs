use super::*;
use crate::test_database::set_record_version;

#[tokio::test]
async fn stored_columns_supply_versions_with_current_body_validation() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.pool().clone());
        let owner = scope(&db.a, "lifecycle-versions").await;
        let tenant = owner.identity.tenant_id;
        let (release, acquisition) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        let event = catalog
            .create_event(
                &owner,
                Event {
                    event_id: EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap(),
                    name: "fixture".into(),
                    due: resolved(&TemporalEngine::new(files.bootstrap.clone())),
                    state: EventState::Scheduled,
                    record_version: TimeVersion::FIRST,
                },
                Utc::now() + chrono::TimeDelta::days(1),
                "fixture".into(),
            )
            .await
            .unwrap();
        let rows = [
            Row::Source(&release.source_id),
            Row::Release(&release.release_id),
            Row::Acquisition(&acquisition.acquisition_id),
            Row::Event(&event.event_id),
        ];
        let mut bodies = Vec::new();
        for row in rows {
            bodies.push(body(&db.a, tenant, row).await);
        }
        for malformed in [
            serde_json::json!(0),
            serde_json::json!(u64::MAX),
            serde_json::json!(-1),
            serde_json::json!("1"),
            serde_json::Value::Null,
        ] {
            for (row, body) in rows.iter().zip(&mut bodies) {
                body["recordVersion"] = malformed.clone();
                set_body(&db.a, tenant, *row, body).await;
            }
            assert!(catalog.source(&owner, &release.source_id).await.is_err());
            assert!(catalog.release(&owner, &release.release_id).await.is_err());
            assert!(
                catalog
                    .acquisition(&owner, &acquisition.acquisition_id)
                    .await
                    .is_err()
            );
            assert!(catalog.event(&owner, &event.event_id).await.is_err());
        }
        // The schema keeps every lifecycle version positive.
        for (row, body) in rows.iter().zip(&mut bodies) {
            body["recordVersion"] = 1.into();
            set_body(&db.a, tenant, *row, body).await;
            let error = set_record_version(&db.a, tenant, *row, 0)
                .await
                .unwrap_err();
            assert!(rejected_by_schema(&error), "{error}");
        }
        assert!(catalog.source(&owner, &release.source_id).await.is_ok());
        assert!(catalog.release(&owner, &release.release_id).await.is_ok());
        assert!(
            catalog
                .acquisition(&owner, &acquisition.acquisition_id)
                .await
                .is_ok()
        );
        assert!(catalog.event(&owner, &event.event_id).await.is_ok());
    })
    .await
    .expect("current lifecycle version qualification exceeded 90 seconds");
}
