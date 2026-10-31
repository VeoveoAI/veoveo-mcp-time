//! Time's App through the hosted router: `ui://time/now` is listed and read under the App
//! MIME type, each admitted by `time.authorities.read`, and exactly the linked tools carry
//! the App link.
use std::collections::BTreeSet;

use super::hosted::{Fixture, app_html, token_granting};
use super::*;
use crate::{
    contract::TimePermission::{self, *},
    mcp::app::{
        APP_DESCRIPTION, APP_ICON, APP_ICON_MIME_TYPE, APP_NAME, APP_TITLE, APP_URI, LINKED_TOOLS,
    },
};
use serde_json::{Value, json};
use veoveo_hub_sdk::apps::{APP_MIME_TYPE, UI_META_KEY};

const TENANT: &str = "tenant-app";

async fn rpc(fixture: &Fixture, grants: &[TimePermission], method: &str, params: Value) -> Value {
    let token = token_granting(TENANT, grants.iter().copied());
    fixture.hub.rpc_with(method, params, Some(&token)).await.1
}

fn listed<'a>(list: &'a Value, member: &str, key: &str, value: &str) -> Option<&'a Value> {
    list["result"][member]
        .as_array()
        .unwrap_or_else(|| panic!("no {member} in {list}"))
        .iter()
        .find(|entry| entry[key] == value)
}

/// Every permission but the one that admits Time's resources.
fn without_read() -> Vec<TimePermission> {
    TimePermission::ALL
        .iter()
        .copied()
        .filter(|permission| *permission != AuthoritiesRead)
        .collect()
}

#[tokio::test]
async fn discovery_lists_the_app_with_its_mime_type_title_description_and_icon() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let resources = rpc(&fixture, &[AuthoritiesRead], "resources/list", json!({})).await;
        let app = listed(&resources, "resources", "uri", APP_URI).expect("the App is listed");
        assert_eq!(
            *app,
            json!({
                "uri": APP_URI,
                "name": APP_NAME,
                "title": APP_TITLE,
                "description": APP_DESCRIPTION,
                "mimeType": APP_MIME_TYPE,
                "icons": [{"src": APP_ICON, "mimeType": APP_ICON_MIME_TYPE}],
                "_meta": {"ui": {}}
            })
        );
        let denied = rpc(&fixture, &without_read(), "resources/list", json!({})).await;
        assert!(
            listed(&denied, "resources", "uri", APP_URI).is_none(),
            "{denied}"
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn reading_the_app_returns_the_packaged_document_under_the_app_mime_type() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let read = rpc(
            &fixture,
            &[AuthoritiesRead],
            "resources/read",
            json!({"uri": APP_URI}),
        )
        .await;
        assert_eq!(
            read["result"]["contents"],
            json!([{"uri": APP_URI, "mimeType": APP_MIME_TYPE, "text": app_html().as_str()}]),
            "{read}"
        );
        let denied = rpc(
            &fixture,
            &without_read(),
            "resources/read",
            json!({"uri": APP_URI}),
        )
        .await;
        assert!(denied.get("result").is_none(), "{denied}");
        assert!(
            denied["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains(AuthoritiesRead.name().as_str())),
            "{denied}"
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn exactly_the_linked_tools_carry_the_app_link() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let tools = rpc(&fixture, TimePermission::ALL, "tools/list", json!({})).await;
        let all = tools["result"]["tools"].as_array().expect("tools listed");
        let linked: BTreeSet<&str> = all
            .iter()
            .filter(|tool| tool["_meta"].get(UI_META_KEY).is_some())
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(linked, LINKED_TOOLS.into_iter().collect::<BTreeSet<_>>());
        assert!(all.len() > LINKED_TOOLS.len());
        let link = json!({"resourceUri": APP_URI, "visibility": ["model", "app"]});
        for name in LINKED_TOOLS {
            let tool = listed(&tools, "tools", "name", name).unwrap();
            assert_eq!(tool["_meta"][UI_META_KEY], link, "{name}");
        }
    })
    .await
    .unwrap();
}
