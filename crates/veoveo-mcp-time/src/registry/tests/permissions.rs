//! Each handler admits exactly the permission Time's requirement table names, from the
//! permissions the Gateway forwards in its access token.
use super::hosted::{Fixture, token_granting};
use super::*;
use crate::contract::TimePermission::{self, *};
use serde_json::json;
use veoveo_sdk_rust::hosting::testing;

const TENANT: &str = "tenant-a";

fn arguments(tool: &str) -> serde_json::Value {
    let inputs: Vec<serde_json::Value> =
        serde_json::from_slice(include_bytes!("../../../testdata/controlled-inputs.json")).unwrap();
    inputs
        .into_iter()
        .find(|case| case["tool"] == tool)
        .unwrap()["arguments"]
        .clone()
}

async fn rpc(
    fixture: &Fixture,
    grants: &[TimePermission],
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let token = token_granting(TENANT, grants.iter().copied());
    fixture
        .gateway
        .rpc_with(method, params, Some(&token))
        .await
        .1
}

/// Calls `tool` as an MCP Task from a client that declares the Tasks extension.
async fn task(fixture: &Fixture, grants: &[TimePermission], tool: &str) -> serde_json::Value {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool,
            "arguments": {},
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": testing::TEST_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientInfo": {"name": "test-gateway", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {
                    "extensions": {"io.modelcontextprotocol/tasks": {}}
                }
            }
        }
    });
    let request = fixture
        .gateway
        .request("/mcp")
        .method("POST")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", testing::TEST_PROTOCOL_VERSION)
        .header("mcp-method", "tools/call")
        .header("mcp-name", tool)
        .header(
            "authorization",
            format!("Bearer {}", token_granting(TENANT, grants.iter().copied())),
        )
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    let (_, text) = fixture.gateway.send(request).await;
    serde_json::from_str(&text).unwrap()
}

fn denied(body: &serde_json::Value, permission: TimePermission) -> bool {
    body["error"]["message"].as_str().is_some_and(|message| {
        message.contains(&format!("Missing permission `{}`", permission.name()))
    })
}

#[tokio::test]
async fn tools_require_the_permission_their_table_entry_names() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        for (tool, required) in [
            ("resolve_time", AuthoritiesRead),
            ("convert_time", AuthoritiesRead),
            ("evaluate_windows", SchedulesExpand),
        ] {
            let others: Vec<_> = TimePermission::ALL
                .iter()
                .copied()
                .filter(|permission| *permission != required)
                .collect();
            let params = json!({"name": tool, "arguments": arguments(tool)});
            let refused = rpc(&fixture, &others, "tools/call", params.clone()).await;
            assert!(denied(&refused, required), "{tool}: {refused}");
            let admitted = rpc(&fixture, &[required], "tools/call", params).await;
            assert!(!denied(&admitted, required), "{tool}: {admitted}");
        }
        let resolved = rpc(
            &fixture,
            &[AuthoritiesRead],
            "tools/call",
            json!({"name": "resolve_time", "arguments": {"expression": {"format": "rfc3339", "value": "2030-01-01T00:00:00Z"}}}),
        )
        .await;
        let due = resolved["result"]["structuredContent"]["instant"].clone();
        assert!(due.is_object(), "{resolved}");
        let create = json!({
            "name": "create_event",
            "arguments": {"name": "launch", "idempotencyKey": "permissions", "due": due}
        });
        let refused = rpc(&fixture, &[AuthoritiesRead], "tools/call", create.clone()).await;
        assert!(denied(&refused, EventsWrite), "{refused}");
        let created = rpc(&fixture, &[EventsWrite], "tools/call", create).await;
        let event = created["result"]["structuredContent"].clone();
        assert!(event["eventId"].is_string(), "{created}");
        let cancel = json!({
            "name": "cancel_event",
            "arguments": {"eventId": event["eventId"], "expectedRecordVersion": event["recordVersion"]}
        });
        let refused = rpc(&fixture, &[AuthoritiesRead], "tools/call", cancel.clone()).await;
        assert!(denied(&refused, EventsWrite), "{refused}");
        let cancelled = rpc(&fixture, &[EventsWrite], "tools/call", cancel).await;
        assert_eq!(cancelled["result"]["structuredContent"]["state"], "cancelled", "{cancelled}");
    })
    .await
    .expect("tool permission checks exceeded 90 seconds");
}

#[tokio::test]
async fn resources_prompts_and_listing_require_reading_authority_data() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let read = json!({"uri": "time://clock/current"});
        let refused = rpc(&fixture, &[EventsWrite], "resources/read", read.clone()).await;
        assert!(denied(&refused, AuthoritiesRead), "{refused}");
        let admitted = rpc(&fixture, &[AuthoritiesRead], "resources/read", read).await;
        assert!(admitted.get("error").is_none(), "{admitted}");

        let prompt = json!({"name": "resolve_operational_time", "arguments": {"expression": "2026-01-01T00:00:00Z"}});
        let refused = rpc(&fixture, &[EventsWrite], "prompts/get", prompt.clone()).await;
        assert!(denied(&refused, AuthoritiesRead), "{refused}");
        let admitted = rpc(&fixture, &[AuthoritiesRead], "prompts/get", prompt).await;
        assert!(!denied(&admitted, AuthoritiesRead), "{admitted}");

        let refused = rpc(&fixture, &[EventsWrite], "resources/list", json!({})).await;
        assert!(denied(&refused, AuthoritiesRead), "{refused}");
        let admitted = rpc(&fixture, &[AuthoritiesRead], "resources/list", json!({})).await;
        assert!(admitted.get("error").is_none(), "{admitted}");
    })
    .await
    .expect("resource permission checks exceeded 90 seconds");
}

#[tokio::test]
async fn task_tools_require_their_permission_before_a_task_exists() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        for (tool, required) in [
            ("expand_schedule", SchedulesExpand),
            ("validate_timeline", TimelinesValidate),
        ] {
            let refused = task(&fixture, &[AuthoritiesRead], tool).await;
            assert!(denied(&refused, required), "{tool}: {refused}");
            // The permission admits the call, which then fails on its empty arguments.
            let admitted = task(&fixture, &[required], tool).await;
            assert!(!denied(&admitted, required), "{tool}: {admitted}");
            assert!(
                admitted["error"]["message"].is_string(),
                "{tool}: {admitted}"
            );
        }
    })
    .await
    .expect("task permission checks exceeded 90 seconds");
}

#[tokio::test]
async fn administration_tools_require_their_administrative_permission() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        // The admitted acquisition downloads in the background. A closed privileged
        // loopback port refuses it at once, so the test never leaves the host.
        let source = |record_version: u64| {
            json!({
                "sourceId": "time-source-0195dabe-7777-7abc-8def-000000000009",
                "name": "IANA tzdb",
                "datasetKind": "tzdb",
                "url": "https://127.0.0.1:9/tzdata-latest.tar.gz",
                "expectedContentType": "application/gzip",
                "enabled": true,
                "recordVersion": record_version
            })
        };
        let resolved = rpc(
            &fixture,
            &[AuthoritiesRead],
            "tools/call",
            json!({"name": "resolve_time", "arguments": {"expression": {"format": "rfc3339", "value": "2030-01-01T00:00:00Z"}}}),
        )
        .await;
        let instant = resolved["result"]["structuredContent"]["instant"].clone();
        assert!(instant.is_object(), "{resolved}");
        let calendar = json!({"calendarId": "calendar-0195dabe-7777-7abc-8def-000000000008",
            "version": 1, "name": "operations", "zoneId": "UTC", "windows": [], "excludedDates": []});
        let cases = [
            ("list_sources", json!({}), AuthoritiesInspect),
            ("get_source", json!({"sourceId": "time-source-0195dabe-7777-7abc-8def-000000000009"}), AuthoritiesInspect),
            (
                "create_source",
                json!({"source": source(0), "idempotencyKey": "administration"}),
                AuthoritiesAdminister,
            ),
            (
                "replace_source",
                json!({"source": source(1), "expectedRecordVersion": 1}),
                AuthoritiesAdminister,
            ),
            ("list_acquisitions", json!({}), AuthoritiesInspect),
            (
                "get_acquisition",
                json!({"acquisitionId": "time-acquisition-00000000-0000-7000-8000-000000000001"}),
                AuthoritiesInspect,
            ),
            (
                "start_acquisition",
                json!({"sourceId": "time-source-0195dabe-7777-7abc-8def-000000000009", "expectedSourceDigest": null, "idempotencyKey": "administration"}),
                AuthoritiesAdminister,
            ),
            (
                "cancel_acquisition",
                json!({"acquisitionId": "time-acquisition-00000000-0000-7000-8000-000000000001"}),
                AuthoritiesAdminister,
            ),
            ("list_active_authorities", json!({}), AuthoritiesInspect),
            (
                "activate_release",
                json!({"releaseId": "time-release-fixture", "expectedReleaseRecordVersion": 1, "expectedActivePointerVersion": 0}),
                AuthoritiesAdminister,
            ),
            (
                "create_calendar",
                json!({"calendar": calendar, "idempotencyKey": "administration"}),
                CalendarsWrite,
            ),
            (
                "create_epoch",
                json!({"epoch": {"epochId": "epoch-launch", "name": "launch", "instant": instant, "version": 1}, "idempotencyKey": "administration"}),
                EpochsWrite,
            ),
            (
                "replace_clock_policy",
                json!({"policy": {"maximumErrorNanoseconds": 100000000, "maximumStratum": 4, "minimumSourceDiversity": 2, "maximumHoldoverSeconds": 300}, "expectedRecordVersion": 0}),
                ClockPoliciesWrite,
            ),
        ];
        for (tool, arguments, required) in cases {
            let params = json!({"name": tool, "arguments": arguments});
            let others: Vec<_> = TimePermission::ALL
                .iter()
                .copied()
                .filter(|permission| *permission != required)
                .collect();
            let refused = rpc(&fixture, &others, "tools/call", params.clone()).await;
            assert!(denied(&refused, required), "{tool}: {refused}");
            let admitted = rpc(&fixture, &[required], "tools/call", params).await;
            assert!(!denied(&admitted, required), "{tool}: {admitted}");
            assert!(admitted.get("result").is_some(), "{tool}: {admitted}");
        }
        // Granted, a registered source is readable by an inspecting operator.
        let read = rpc(
            &fixture,
            &[AuthoritiesInspect],
            "tools/call",
            json!({"name": "get_source", "arguments": {"sourceId": "time-source-0195dabe-7777-7abc-8def-000000000009"}}),
        )
        .await;
        assert_eq!(read["result"]["structuredContent"]["sourceId"], "time-source-0195dabe-7777-7abc-8def-000000000009", "{read}");
        let missing = rpc(
            &fixture,
            &[AuthoritiesInspect],
            "tools/call",
            json!({"name": "get_source", "arguments": {"sourceId": "time-source-0195dabe-7777-7abc-8def-00000000000a"}}),
        )
        .await;
        assert_eq!(missing["result"]["isError"], true, "{missing}");
    })
    .await
    .expect("administrative permission checks exceeded 90 seconds");
}
