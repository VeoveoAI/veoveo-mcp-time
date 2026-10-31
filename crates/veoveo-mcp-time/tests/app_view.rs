//! Time's App document, `assets/now.html`: the packaged view loads through the SDK's
//! `AppHtml` within the host's bound, keeps to its own Content Security Policy, names no
//! network origin, speaks the bridge methods the Apps contract assigns to a view, takes
//! MCP's own resource notifications as its wakes, ticks from the monotonic clock without a
//! wall clock or an interval timer, and names only the tools linked to it.
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
        "notifications/subscriptions/acknowledged",
        "notifications/resources/updated",
        "notifications/cancelled",
    ] {
        assert!(html.contains(&format!("\"{method}\"")), "{method}");
    }
    // The host forwards MCP's own notifications; the ui/ namespace carries no wake.
    assert!(!html.contains("ui/notifications/resource-updated"));
    // A host that does not forward the listen answers Method not found.
    assert!(html.contains("const METHOD_NOT_FOUND = -32601;"));
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

#[test]
fn the_clock_ticks_from_the_monotonic_clock_and_reads_only_on_wakes_and_refresh() {
    let html = document();
    // Elapsed time comes from performance.now(), never from the browser's wall clock.
    assert!(html.contains("performance.now()"));
    for wall in ["Date.now(", "new Date()"] {
        assert!(
            !html.contains(wall),
            "the view reads the wall clock with {wall}"
        );
    }
    // Nothing repeats on an interval; the tick is one timeout per displayed second.
    assert!(!html.contains("setInterval"));
    assert_eq!(html.matches("tickTimer = setTimeout(renderTick").count(), 1);
    // The tick bounds advancing UTC and local time by the holds Time states.
    for hold in ["utcOffsetHoldsUntil", "offsetHoldsUntil"] {
        assert!(html.contains(&format!(".{hold}")), "{hold}");
    }
    // Reads happen in the loaders, which only startup, wakes, acknowledgements and Refresh
    // start.
    assert_eq!(html.matches("request(\"resources/read\"").count(), 1);
    assert_eq!(html.matches("refreshAll();").count(), 2);
    assert!(function(&html, "acknowledged").contains("refreshAll();"));
    assert!(html.contains("$(\"refresh\").addEventListener(\"click\", refreshAll);"));
}

#[test]
fn every_control_is_a_button_or_input_with_a_visible_focus_ring() {
    let html = document();
    assert!(html.contains(":focus-visible { outline: 2px solid var(--accent);"));
    assert!(!html.contains("tabindex"));
    for control in ["<a ", "onclick=", "contenteditable"] {
        assert!(!html.contains(control), "{control}");
    }
}

/// The body of the view's function `name`, up to the closing brace at its own indent.
fn function<'a>(html: &'a str, name: &str) -> &'a str {
    let start = html
        .find(&format!("  function {name}("))
        .unwrap_or_else(|| panic!("the view defines {name}"));
    let end = html[start..]
        .find("\n  }\n")
        .unwrap_or_else(|| panic!("{name} ends"));
    &html[start..start + end]
}

#[test]
fn an_ended_listen_is_listened_again_with_a_bounded_backoff_except_for_method_not_found() {
    let html = document();
    assert!(html.contains("const LISTEN_RETRY_FIRST_MS = 1000;"));
    assert!(html.contains("const LISTEN_RETRY_MAX_MS = 30000;"));
    // Each listen is a fresh request; one listen is open at a time.
    let listen = function(&html, "listen");
    assert!(listen.contains("send(\"subscriptions/listen\""));
    assert!(listen.contains("state.listenId = id;"));
    // An end the view did not cause backs off, doubling up to the ceiling, and listens again.
    let ended = function(&html, "listenEnded");
    assert!(ended.contains("state.tornDown || state.listenId !== id"));
    assert!(ended.contains("Math.min(delay * 2, LISTEN_RETRY_MAX_MS)"));
    let retry = ended.find("setTimeout(listen, delay)").expect("a relisten");
    assert!(ended.contains("setLive(\"reconnecting\""));
    // A host that answers -32601 forwards no listens: the view stops before any retry.
    let refused = ended.find("METHOD_NOT_FOUND").expect("a -32601 branch");
    let stop = refused
        + ended[refused..]
            .find("return;")
            .expect("the branch returns");
    assert!(stop < retry);
    assert!(ended[refused..stop].contains("Live updates are off"));
    // An acknowledgement resets the backoff, marks the view live and rereads everything.
    let acknowledged = function(&html, "acknowledged");
    assert!(acknowledged.contains("state.retryDelay = LISTEN_RETRY_FIRST_MS;"));
    assert!(acknowledged.contains("setLive(\"live\")"));
    assert!(html.contains("reconnecting: \"Reconnecting…\""));
    // Teardown cancels the open listen and any pending relisten.
    let teardown = function(&html, "teardown");
    assert!(teardown.contains("notifications/cancelled"));
    assert!(teardown.contains("clearTimeout(state.retryTimer);"));
}
