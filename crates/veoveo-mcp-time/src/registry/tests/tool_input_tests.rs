//! Hosted argument admission using the existing isolated authority fixture.
use super::hosted::token;
use super::*;
use serde_json::json;
use veoveo_hub_sdk_fixture::tool_inputs as input_fixture;

fn retired_members(wire: &serde_json::Value) -> Vec<serde_json::Value> {
    fn collect(value: &serde_json::Value, path: &str, keys: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, child) in object {
                    if key.bytes().any(|b| b.is_ascii_uppercase()) {
                        keys.push((path.into(), key.clone()));
                    }
                    collect(child, &format!("{path}/{key}"), keys);
                }
            }
            serde_json::Value::Array(values) => {
                for (i, child) in values.iter().enumerate() {
                    collect(child, &format!("{path}/{i}"), keys);
                }
            }
            _ => {}
        }
    }
    let mut keys = vec![];
    collect(wire, "", &mut keys);
    let mut controls = vec![];
    for (path, key) in keys {
        let retired: String = key
            .chars()
            .flat_map(|c| {
                if c.is_ascii_uppercase() {
                    vec!['_', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect();
        for mixed in [false, true] {
            let mut bad = wire.clone();
            let object = bad.pointer_mut(&path).unwrap().as_object_mut().unwrap();
            let value = object[&key].clone();
            if !mixed {
                object.remove(&key);
            }
            object.insert(retired.clone(), value);
            controls.push(bad);
        }
    }
    controls
}

fn rejects_arguments(tool: &str, arguments: &serde_json::Value) -> bool {
    match tool {
        "resolve_time" => serde_json::from_value::<ResolveTimeRequest>(arguments.clone()).is_err(),
        "convert_time" => serde_json::from_value::<ConvertTimeRequest>(arguments.clone()).is_err(),
        "evaluate_windows" => {
            serde_json::from_value::<EvaluateWindowsRequest>(arguments.clone()).is_err()
        }
        "expand_schedule" => {
            serde_json::from_value::<ExpandScheduleRequest>(arguments.clone()).is_err()
        }
        _ => panic!("unexpected fixture tool"),
    }
}

#[tokio::test]
async fn unknown_tool_arguments_complete_before_time_resolution() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let fixture = super::hosted::Fixture::new().await;
        let hub = &fixture.hub;
        let state = &fixture.state;
        let discover = hub.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let tools = hub.rpc("tools/list", json!({})).await;
        assert!(tools.get("error").is_none(), "{tools}");
        // Real authenticated HTTP captures remain snake_case while owner JSON is camelCase.
        let owner_token = token("tenant-a");
        let inputs: Vec<serde_json::Value> =
            serde_json::from_slice(include_bytes!("../../../testdata/controlled-inputs.json"))
                .unwrap();
        let calendar: OperationalCalendar = serde_json::from_value(
            inputs
                .iter()
                .find(|case| case["tool"] == "expand_schedule")
                .unwrap()["arguments"]["calendar"]
                .clone(),
        )
        .unwrap();
        // Decoder corpus IDs exercise the public lexical profile. This native
        // fixture must use the catalog's admitted UUIDv7 record identity.
        let mut calendar: crate::contract::OperationalCalendarValue = calendar.into();
        calendar.calendar_id =
            crate::contract::CalendarId::parse("calendar-0195dabe-7777-7abc-8def-000000000007")
                .unwrap();
        let calendar = calendar.build().unwrap();
        let create = CreateCalendarRequestValue {
            calendar: calendar.clone(),
            idempotency_key: "calendar-fixture".into(),
        }
        .build()
        .unwrap();
        let (_, created) = hub
            .rpc_with(
                "tools/call",
                json!({"name": "create_calendar", "arguments": create}),
                Some(&owner_token),
            )
            .await;
        assert_eq!(created["result"]["isError"], false, "{created}");
        let published: OperationalCalendar =
            serde_json::from_value(created["result"]["structuredContent"].clone()).unwrap();
        assert_eq!(published, calendar);
        let uri = format!(
            "time://calendars/{}/versions/{}",
            calendar.calendar_id,
            calendar.version.get()
        );
        for (uri, bearer, readable) in [
            (uri.clone(), owner_token.clone(), true),
            (
                "time://calendars/epoch-wrong/versions/1".into(),
                owner_token.clone(),
                false,
            ),
            (
                format!("time://calendars/{}/versions/0", calendar.calendar_id),
                owner_token.clone(),
                false,
            ),
            (
                format!(
                    "time://calendars/{}/versions/9223372036854775808",
                    calendar.calendar_id
                ),
                owner_token.clone(),
                false,
            ),
            (uri, token("tenant-b"), false),
        ] {
            let (_, read) = hub
                .rpc_with("resources/read", json!({"uri": uri}), Some(&bearer))
                .await;
            assert_eq!(read.get("error").is_none(), readable, "{uri}: {read}");
            if readable {
                let text = read["result"]["contents"][0]["text"].as_str().unwrap();
                assert_eq!(
                    serde_json::from_str::<OperationalCalendar>(text).unwrap(),
                    calendar
                );
            }
        }
        let (_, clock) = hub
            .rpc_with(
                "resources/read",
                json!({"uri":"time://clock/current"}),
                Some(&owner_token),
            )
            .await;
        assert!(clock.get("error").is_none(), "{clock}");
        let clock_text = clock["result"]["contents"][0]["text"].as_str().unwrap();
        let current: ClockCurrent = serde_json::from_str(clock_text).unwrap();
        let wire = serde_json::to_value(&current).unwrap();
        assert_eq!(
            serde_json::from_value::<ClockCurrent>(wire.clone()).unwrap(),
            current
        );
        let schema = serde_json::to_value(schemars::schema_for!(ClockCurrent)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&wire));
        assert!(wire.get("effectivePolicy").is_some() && wire.get("clockQuality").is_some());
        for (current, retired) in [
            ("effectivePolicy", "effective_policy"),
            ("clockQuality", "clock_quality"),
        ] {
            assert!(wire.get(retired).is_none());
            for mixed in [false, true] {
                let mut bad = wire.clone();
                bad[retired] = bad[current].clone();
                if !mixed {
                    bad.as_object_mut().unwrap().remove(current);
                }
                assert!(!validator.is_valid(&bad));
                assert!(serde_json::from_value::<ClockCurrent>(bad).is_err());
            }
        }
        let listed = hub.rpc("prompts/list", json!({})).await;
        assert!(listed.get("error").is_none(), "{listed}");
        for (name, arguments, argument, retired) in [
            (
                "resolve_operational_time",
                json!({"expression":"2026-01-01T00:00:00Z","zone_id":"UTC"}),
                "zone_id",
                "zoneId",
            ),
            (
                "expand_operational_calendar",
                json!({"calendar_id":"calendar-fixture","version":"1","horizon":"[0,100)"}),
                "calendar_id",
                "calendarId",
            ),
        ] {
            let declaration = listed["result"]["prompts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|prompt| prompt["name"] == name)
                .unwrap();
            assert!(
                declaration["arguments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value["name"] == argument)
            );
            assert!(
                !declaration["arguments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value["name"] == retired)
            );
            let result = hub
                .rpc("prompts/get", json!({"name":name,"arguments":arguments}))
                .await;
            assert!(result.get("error").is_none(), "{result}");
            assert!(!result["result"]["messages"].as_array().unwrap().is_empty());
            for mixed in [false, true] {
                let mut bad = arguments.clone();
                bad[retired] = bad[argument].clone();
                if !mixed {
                    bad.as_object_mut().unwrap().remove(argument);
                }
                let refused = hub
                    .rpc("prompts/get", json!({"name":name,"arguments":bad}))
                    .await;
                assert_eq!(refused["error"]["code"], -32602, "{refused}");
            }
        }
        let mut arguments =
            json!({"expression":{"format":"rfc3339","value":"2026-01-01T00:00:00Z"}});
        let _: ResolveTimeRequest = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = hub
            .rpc(
                "tools/call",
                json!({"name":"resolve_time","arguments":arguments}),
            )
            .await;
        assert!(body.get("error").is_none(), "{body}");
        assert_eq!(body["result"]["resultType"], "complete", "{body}");
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(body["result"].clone()).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            serde_json::to_string(&result.content)
                .unwrap()
                .contains("undeclared")
        );
        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 30);
        for case in cases {
            let tool = tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| tool["name"] == case.tool)
                .unwrap();
            let validator = jsonschema::validator_for(&tool["inputSchema"]).unwrap();
            assert!(
                validator.is_valid(&case.arguments),
                "{}: published schema refuses the baseline",
                case.branch
            );
            match case.tool.as_str() {
                "resolve_time" => {
                    let _: crate::contract::ResolveTimeRequest = case.decode();
                }
                "convert_time" => {
                    let _: crate::contract::ConvertTimeRequest = case.decode();
                }
                "evaluate_windows" => {
                    let _: crate::contract::EvaluateWindowsRequest = case.decode();
                }
                "expand_schedule" => {
                    let _: crate::contract::ExpandScheduleRequest = case.decode();
                }
                _ => panic!("unexpected fixture tool"),
            }
            for arguments in retired_members(&case.arguments) {
                let body = hub
                    .rpc(
                        "tools/call",
                        json!({"name":case.tool,"arguments":arguments}),
                    )
                    .await;
                assert!(body.get("error").is_none(), "{}: {body}", case.branch);
                assert_eq!(
                    body["result"]["resultType"], "complete",
                    "{}: {body}",
                    case.branch
                );
                let result: rmcp::model::CallToolResult =
                    serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(result.is_error, Some(true), "{}: {body}", case.branch);
                assert!(state.tasks.claim_next().await.unwrap().is_none());
            }
            for (location, arguments, unknown_field) in case
                .unknown_fields()
                .into_iter()
                .map(|(location, arguments)| (location, arguments, true))
                .chain(
                    case.invalid_values()
                        .into_iter()
                        .map(|(location, arguments)| (location, arguments, false)),
                )
            {
                assert!(
                    rejects_arguments(&case.tool, &arguments),
                    "{} {location}",
                    case.branch
                );
                let schema_refuses = !validator.is_valid(&arguments);
                if unknown_field {
                    assert!(
                        schema_refuses,
                        "{} {location}: published schema admits an unknown field",
                        case.branch
                    );
                }
                let body = hub
                    .rpc(
                        "tools/call",
                        json!({"name":case.tool,"arguments":arguments}),
                    )
                    .await;
                assert!(
                    body.get("error").is_none(),
                    "{} {location}: {body}",
                    case.branch
                );
                assert_eq!(
                    body["result"]["resultType"], "complete",
                    "{} {location}: {body}",
                    case.branch
                );
                let result: rmcp::model::CallToolResult =
                    serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(
                    result.is_error,
                    Some(true),
                    "{} {location}: {body}",
                    case.branch
                );
                // Typed routes mask owner decoding details. Tagged oneOf schemas
                // may identify the containing expression rather than its field.
                let content = serde_json::to_string(&result.content).unwrap();
                assert!(
                    content.contains("invalid typed tool arguments"),
                    "{} {location}: {body}",
                    case.branch
                );
                if schema_refuses {
                    assert!(
                        content.contains(&format!(
                            "The arguments do not match the published input schema of `{}`",
                            case.tool
                        )),
                        "{} {location}: {body}",
                        case.branch
                    );
                }
            }
        }
        assert!(state.tasks.claim_next().await.unwrap().is_none());
    })
    .await
    .expect("Time argument admission exceeded 120 seconds");
}
