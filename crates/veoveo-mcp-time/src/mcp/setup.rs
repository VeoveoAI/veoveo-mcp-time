use std::sync::LazyLock;

use rmcp::model::{ServerCapabilities, ServerConfig};
use veoveo_hub_sdk::{
    docs::ServerDocs,
    server_contract::{McpServerContract, McpServerSetup},
};
use veoveo_types::ResourceScheme;

use super::SERVER_DOCS;
use crate::contract::{TimeProvider, TimeScope};

pub struct TimeContract;
pub(crate) static SERVER_SETUP: LazyLock<McpServerSetup<TimeContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Time MCP contract setup"));

impl McpServerContract for TimeContract {
    type Scope = TimeScope;
    type Server = super::TimeMcp;
    type Provider = TimeProvider;

    fn scheme() -> ResourceScheme {
        ResourceScheme::parse("time").expect("declared Time scheme")
    }
    fn scopes() -> &'static [TimeScope] {
        TimeScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
}

fn server_info() -> ServerConfig {
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_prompts()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_completions()
        .build();
    capabilities.extensions.get_or_insert_default().insert(
        rmcp::model::TASKS_EXTENSION_ID.to_owned(),
        rmcp::model::JsonObject::new(),
    );
    if let Some(registration) = super::declarations::REGISTRATION.get() {
        veoveo_hub_sdk::registration::mcp::advertise(
            &mut capabilities,
            registration,
            &super::requirements::AUTHORIZATION,
        );
    }
    let mut info = ServerConfig::default();
    info.capabilities = capabilities;
    info.server_info = rmcp::model::Implementation::new("time", env!("CARGO_PKG_VERSION"));
    info.instructions = Some("Time interpretation and scheduling for agents. Resolve civil, military, GNSS, Unix, TAI, and mission-relative times against versioned TZDB and leap-second releases. Call `expand_schedule` and `validate_timeline` as MCP Tasks. When you pass a time to Map or Optimization, pass the resolved TimeInstant with its uncertainty, not a plain string.".to_owned());
    info
}
