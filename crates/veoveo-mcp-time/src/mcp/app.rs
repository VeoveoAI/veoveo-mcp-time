//! Time's App, `ui://time/now`: its descriptor, the packaged document it serves and the
//! tools linked to it. The typed routes declare both, so discovery, reads, registration
//! requirements and the UI capability derive from them.
use rmcp::model::Icon;
use veoveo_hub_sdk::{
    apps::{AppUri, UiVisibility},
    calls::ToolCall,
    hosting::{AppBuilder, RoutePolicy, RoutesBuilder},
};

use super::TimeMcp;
use crate::contract::{
    ConvertTimeRequest, ExpandScheduleRequest, ResolveTimeRequest, TimePermission,
};

/// The App's address.
pub const APP_URI: &str = "ui://time/now";
/// The App's programmatic resource name.
pub const APP_NAME: &str = "now";
pub const APP_TITLE: &str = "Time";
pub const APP_DESCRIPTION: &str = "The current authoritative time in your zone with its UTC, military, GPS and TAI forms and clock quality, your upcoming events, your calendars' next seven days, and a resolver for any time expression.";
/// A clock, as an SVG `data:` URI.
pub const APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCIgZmlsbD0ibm9uZSIgc3Ryb2tlPSIjNmI3MjgwIiBzdHJva2Utd2lkdGg9IjIiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIgc3Ryb2tlLWxpbmVqb2luPSJyb3VuZCI+PGNpcmNsZSBjeD0iMTIiIGN5PSIxMiIgcj0iOSIvPjxwYXRoIGQ9Ik0xMiA3djVsMyAyIi8+PC9zdmc+";
pub const APP_ICON_MIME_TYPE: &str = "image/svg+xml";
/// The tools the view invokes; every other Time tool carries no App link.
pub const LINKED_TOOLS: [&str; 3] = [
    ResolveTimeRequest::TOOL,
    ConvertTimeRequest::TOOL,
    ExpandScheduleRequest::TOOL,
];
/// Who may invoke a linked tool: the model, as before, and the App.
pub const LINKED_VISIBILITY: [UiVisibility; 2] = [UiVisibility::Model, UiVisibility::App];

pub fn uri() -> AppUri {
    AppUri::parse(APP_URI).expect("Time's App address")
}

/// Declare the App, admitted by `time.authorities.read` as every Time resource is, and
/// link its tools.
pub(super) fn register(routes: RoutesBuilder<TimeMcp>) -> RoutesBuilder<TimeMcp> {
    let policy =
        RoutePolicy::permission(TimePermission::AuthoritiesRead).expect("Time read permission");
    routes
        .app(
            AppBuilder::new(uri(), APP_NAME)
                .title(APP_TITLE)
                .description(APP_DESCRIPTION)
                .icon(Icon::new(APP_ICON).with_mime_type(APP_ICON_MIME_TYPE))
                .policy(policy)
                .reader(|domain: TimeMcp, _| async move { Ok(domain.app.clone()) }),
        )
        .app_tools(&uri(), &LINKED_TOOLS, &LINKED_VISIBILITY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{SERVER_SETUP, TimeMcp};
    use veoveo_hub_sdk::{
        apps::{APP_MIME_TYPE, EXTENSION_ID},
        hosting::DomainServer,
        registration::RouteAccess,
    };
    use veoveo_types::ResourceUri;

    #[test]
    fn initialize_declares_the_ui_extension_with_the_app_mime_type() {
        let extensions = SERVER_SETUP
            .server_config()
            .capabilities
            .extensions
            .clone()
            .expect("Time declares extensions");
        assert_eq!(
            serde_json::Value::Object(extensions.get(EXTENSION_ID).cloned().unwrap()),
            serde_json::json!({"mimeTypes": [APP_MIME_TYPE]})
        );
    }

    #[test]
    fn registration_requires_the_read_permission_for_the_app() {
        let authorization = TimeMcp::routes().authorization();
        assert_eq!(
            authorization
                .requirements
                .resources
                .get(&ResourceUri::new(APP_URI).unwrap()),
            Some(&RouteAccess::Permission(
                TimePermission::AuthoritiesRead.name().clone()
            ))
        );
    }

    #[test]
    fn the_icon_is_an_svg_data_uri() {
        assert!(APP_ICON.starts_with("data:image/svg+xml;base64,"));
        assert_eq!(uri().server().as_str(), "time");
    }
}
