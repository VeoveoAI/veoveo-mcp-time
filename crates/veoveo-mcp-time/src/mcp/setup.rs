use std::sync::LazyLock;

use rmcp::model::{ServerCapabilities, ServerConfig};
use veoveo_hub_sdk::{
    docs::ServerDocs,
    server_contract::{McpServerContract, McpServerSetup},
    skills::ServerSkills,
};
use veoveo_types::ResourceScheme;

use super::SERVER_DOCS;
use crate::contract::{TimeProvider, TimeScope};

/// The skills under `skills/`, embedded at build time and served at `skill://time/…`.
pub(crate) static SERVER_SKILLS: LazyLock<ServerSkills> =
    LazyLock::new(|| veoveo_hub_sdk::server_skills!("time"));

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
    fn skills() -> &'static ServerSkills {
        &SERVER_SKILLS
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
    info.instructions = Some(INSTRUCTIONS.to_owned());
    info
}

/// What the server is and which skills say how to use it.
const INSTRUCTIONS: &str = "Time is the temporal authority: it resolves civil, military, \
     GNSS, Unix, TAI and mission-relative times against versioned time-zone and \
     leap-second releases. Its skills say how: skill://time/resolve-a-time/SKILL.md, \
     skill://time/expand-a-calendar/SKILL.md, skill://time/validate-a-timeline/SKILL.md \
     and skill://time/update-time-authorities/SKILL.md.";

#[cfg(test)]
mod tests {
    #[test]
    fn the_instructions_name_every_skill_the_server_serves() {
        let skills = &*super::SERVER_SKILLS;
        assert_eq!(skills.skills().len(), 4);
        for skill in skills.skills() {
            assert!(super::INSTRUCTIONS.contains(&skill.uri), "{}", skill.uri);
        }
    }
}
