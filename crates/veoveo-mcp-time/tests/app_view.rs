//! Time's App document, `assets/now.html`: the packaged view loads through the SDK's
//! `AppHtml` within the host's bound, keeps to its own Content Security Policy, names no
//! network origin, speaks the bridge methods the Apps contract assigns to a view and names
//! only the tools linked to it.
use std::path::Path;

use veoveo_hub_sdk::{
    apps::{AppHtml, MAX_APP_HTML_BYTES},
    calls::ToolCall,
    definition::CollectionTool,
};
use veoveo_mcp_time::contract::*;

fn document() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/now.html");
    AppHtml::load(&path)
        .expect("the packaged App document loads")
        .as_str()
        .to_owned()
}

/// The tools linked to `ui://time/now`, as Time's routes link them.
const LINKED: [&str; 3] = veoveo_mcp_time::mcp::app::LINKED_TOOLS;

/// Every other Time tool: none carries an App link, so the view names none.
const UNLINKED: [&str; 18] = [
    EvaluateWindowsRequest::TOOL,
    AssessClockRequest::TOOL,
    ValidateTimelineRequest::TOOL,
    CreateEventRequest::TOOL,
    CancelEventRequest::TOOL,
    ListSources::TOOL,
    GetSourceRequest::TOOL,
    CreateSourceRequest::TOOL,
    ReplaceSourceRequest::TOOL,
    ListAcquisitions::TOOL,
    GetAcquisitionRequest::TOOL,
    StartAcquisitionRequest::TOOL,
    CancelAcquisitionRequest::TOOL,
    ListActiveAuthorities::TOOL,
    ActivateReleaseRequest::TOOL,
    CreateCalendarRequest::TOOL,
    UpsertEpochRequest::TOOL,
    ReplaceClockQualityPolicyRequest::TOOL,
];

#[test]
fn the_packaged_view_is_one_html_document_within_the_app_bound() {
    let html = document();
    assert!(html.len() as u64 <= MAX_APP_HTML_BYTES);
    assert!(html.starts_with("<!doctype html>\n"));
    assert_eq!(html.matches("<script>").count(), 1);
    assert_eq!(html.matches("<style>").count(), 1);
}

#[test]
fn the_view_allows_only_its_inline_script_and_style() {
    let html = document();
    assert_eq!(html.matches("Content-Security-Policy").count(), 1);
    assert!(html.contains(
        r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'">"#
    ));
}

#[test]
fn the_view_names_no_network_origin_and_loads_nothing() {
    let html = document();
    for (index, _) in html.match_indices("://") {
        let scheme: String = html[..index]
            .chars()
            .rev()
            .take_while(|character| character.is_ascii_alphanumeric())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        assert!(
            matches!(scheme.as_str(), "time" | "ui"),
            "the view names a `{scheme}://` address"
        );
    }
    for loader in [
        " src=",
        " href=",
        "<link",
        "<iframe",
        "<form",
        "@import",
        "url(",
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
        "EventSource",
        "sendBeacon",
        "import(",
        "window.open",
        "localStorage",
        "document.cookie",
    ] {
        assert!(!html.contains(loader), "the view uses `{loader}`");
    }
}

#[test]
fn the_view_speaks_the_bridge_a_view_owes_its_host() {
    let html = document();
    for method in [
        "ui/initialize",
        "ui/notifications/initialized",
        "ui/notifications/host-context-changed",
        "ui/request-display-mode",
        "ui/notifications/size-changed",
        "ui/resource-teardown",
        "resources/read",
        "tools/call",
        "tasks/get",
        "tasks/cancel",
        "subscriptions/listen",
        "ui/notifications/resource-updated",
        "notifications/cancelled",
    ] {
        assert!(html.contains(&format!("\"{method}\"")), "{method}");
    }
    assert!(html.contains(r#"const APPS_PROTOCOL = "2026-01-26";"#));
    assert!(html.contains(r#"const DISPLAY_MODES = ["inline", "fullscreen"];"#));
    assert!(html.contains("event.source !== window.parent"));
}

#[test]
fn the_view_names_only_its_linked_tools() {
    let html = document();
    for tool in LINKED {
        assert!(html.contains(&format!("\"{tool}\"")), "{tool}");
    }
    for tool in UNLINKED {
        assert!(!html.contains(tool), "the view names unlinked {tool}");
    }
}
