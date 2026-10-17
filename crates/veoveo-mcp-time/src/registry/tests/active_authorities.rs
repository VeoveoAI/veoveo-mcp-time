//! An authenticated operator uses the active pointer guard for the next activation.
use super::hosted::{Fixture, token};
use super::*;

/// Calls `tool` through the hosted router and returns its structured content.
async fn call(
    hub: &veoveo_hub_sdk::hosting::testing::TestHub,
    token: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let (_, body) = hub
        .rpc_with(
            "tools/call",
            serde_json::json!({"name": tool, "arguments": arguments}),
            Some(token),
        )
        .await;
    assert_eq!(body["result"]["isError"], false, "{body}");
    body["result"]["structuredContent"].clone()
}

#[tokio::test]
async fn operator_reads_pointer_guard_for_next_activation() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let (hub, state, files, db) = (&fixture.hub, &fixture.state, &fixture.files, &fixture.db);
        let owner_token = token("tenant-a");
        // The operator reads the actual pointer guard, rather than the release's
        // independently advancing record version, for the next activation.
        let empty: AdminPage<crate::ActiveAuthoritySelection, crate::NoCursor> =
            serde_json::from_value(
                call(
                    hub,
                    &owner_token,
                    "list_active_authorities",
                    serde_json::json!({}),
                )
                .await,
            )
            .unwrap();
        assert!(empty.items.is_empty());
        let operator = scope(&db.a, "tenant-a").await;
        let mut guard = TimeWriteGuard::Absent;
        for version in 1..=2 {
            let (candidate, _) = stage(
                &state.catalog,
                &operator,
                AuthorityDatasetKind::Tzdb,
                &files.tzdb,
            )
            .await;
            let activation = ActivateReleaseRequest {
                release_id: candidate.release_id.clone(),
                expected_release_record_version: candidate.record_version,
                expected_active_pointer_version: guard,
            };
            call(
                hub,
                &owner_token,
                "activate_release",
                serde_json::to_value(&activation).unwrap(),
            )
            .await;
            let selected: AdminPage<crate::ActiveAuthoritySelection, crate::NoCursor> =
                serde_json::from_value(
                    call(
                        hub,
                        &owner_token,
                        "list_active_authorities",
                        serde_json::json!({}),
                    )
                    .await,
                )
                .unwrap();
            assert_eq!(selected.items.len(), 1);
            let selected = &selected.items[0];
            assert_eq!(selected.pointer_version.get(), version);
            assert_eq!(selected.release.record_version.get(), 2);
            assert_eq!(selected.release.release_id, candidate.release_id);
            guard = selected.write_guard();
        }
        let other: AdminPage<crate::ActiveAuthoritySelection, crate::NoCursor> =
            serde_json::from_value(
                call(
                    hub,
                    &token("tenant-b"),
                    "list_active_authorities",
                    serde_json::json!({}),
                )
                .await,
            )
            .unwrap();
        assert!(other.items.is_empty());
    })
    .await
    .expect("Time active selection control exceeded 90 seconds");
}
