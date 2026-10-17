//! The one table that says which permission each of Time's targets requires. It
//! builds the `ai.veoveo/authorization` declaration the Hub evaluates, and every
//! handler checks the same declaration against the permissions the Hub granted.
use std::sync::LazyLock;

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_hub_sdk::registration::{
    AuthorizationBuilder, AuthorizationDeclaration, DeclarationError, RequirementTarget,
};
use veoveo_hub_sdk::{HubInternalIdentity, hosting::hub_identity};
use veoveo_hub_sdk::{LocalToolName, PromptName};
use veoveo_types::{PermissionName, ResourceTemplateUri, ResourceUri};

use super::setup::TimeContract;
use crate::{contract::TimePermission, prompts::TimePrompt, uris};
use veoveo_hub_sdk::server_contract::McpServerContract;

use TimePermission::*;

/// Every tool and the permission it requires. Task tools are listed once: the
/// same requirement admits the call and the Task it creates.
pub(crate) const TOOLS: &[(&str, TimePermission)] = &[
    ("resolve_time", AuthoritiesRead),
    ("convert_time", AuthoritiesRead),
    ("assess_clock", AuthoritiesRead),
    ("evaluate_windows", SchedulesExpand),
    ("expand_schedule", SchedulesExpand),
    ("validate_timeline", TimelinesValidate),
    ("create_event", EventsWrite),
    ("cancel_event", EventsWrite),
    ("list_sources", AuthoritiesInspect),
    ("get_source", AuthoritiesInspect),
    ("create_source", AuthoritiesAdminister),
    ("replace_source", AuthoritiesAdminister),
    ("list_acquisitions", AuthoritiesInspect),
    ("get_acquisition", AuthoritiesInspect),
    ("start_acquisition", AuthoritiesAdminister),
    ("cancel_acquisition", AuthoritiesAdminister),
    ("list_active_authorities", AuthoritiesInspect),
    ("activate_release", AuthoritiesAdminister),
    ("create_calendar", CalendarsWrite),
    ("create_epoch", EpochsWrite),
    ("replace_clock_policy", ClockPoliciesWrite),
];

/// Every fixed resource. Reads, subscriptions and the list of a collection's first
/// page require reading authority data.
pub(crate) const RESOURCES: &[(&str, TimePermission)] = &[
    (uris::DOCS_URI, AuthoritiesRead),
    (uris::CONTRACT_URI, AuthoritiesRead),
    (uris::CLOCK_CURRENT_URI, AuthoritiesRead),
    (uris::CLOCK_QUALITY_URI, AuthoritiesRead),
    (uris::AUTHORITIES_CURRENT_URI, AuthoritiesRead),
    (uris::CALENDARS_URI, AuthoritiesRead),
    (uris::EPOCHS_URI, AuthoritiesRead),
    (uris::EVENTS_URI, AuthoritiesRead),
    (uris::AUTHORITY_RELEASES_URI, AuthoritiesRead),
    (uris::BOOTSTRAP_AUTHORITIES_URI, AuthoritiesRead),
];

/// Every parameterized resource, as RFC 6570 simple templates that match the concrete
/// URIs Time serves. A collection's later pages carry their cursor as a query.
pub(crate) const RESOURCE_TEMPLATES: &[(&str, TimePermission)] = &[
    ("time://docs/{doc_id}", AuthoritiesRead),
    ("time://zones/{zone_id}", AuthoritiesRead),
    ("time://authorities/releases/{release_id}", AuthoritiesRead),
    (
        "time://authorities/releases?cursor={cursor}",
        AuthoritiesRead,
    ),
    ("time://authorities/bootstrap/{release_id}", AuthoritiesRead),
    (
        "time://authorities/bootstrap?cursor={cursor}",
        AuthoritiesRead,
    ),
    (
        "time://calendars/{calendar_id}/versions/{version}",
        AuthoritiesRead,
    ),
    ("time://calendars?cursor={cursor}", AuthoritiesRead),
    ("time://epochs/{epoch_id}", AuthoritiesRead),
    (
        "time://epochs/{epoch_id}/versions/{version}",
        AuthoritiesRead,
    ),
    ("time://epochs?cursor={cursor}", AuthoritiesRead),
    ("time://events/{event_id}", AuthoritiesRead),
    ("time://events?cursor={cursor}", AuthoritiesRead),
];

/// Every prompt renders guidance over resolved times.
pub(crate) const PROMPTS: TimePermission = AuthoritiesRead;

/// Listing resources and completing arguments read Time's catalogs.
pub(crate) const SERVER: TimePermission = AuthoritiesRead;

/// Time's authorization declaration, checked against its namespace as the Hub
/// checks it.
pub(crate) static AUTHORIZATION: LazyLock<AuthorizationDeclaration> =
    LazyLock::new(|| declaration().expect("Time's requirement table forms a valid declaration"));

pub(crate) fn declaration() -> Result<AuthorizationDeclaration, DeclarationError> {
    let invalid = |field: &'static str| DeclarationError::Invalid {
        field,
        reason: "invalid Time target spelling",
    };
    let mut builder = AuthorizationBuilder::<TimePermission>::new();
    for (tool, permission) in TOOLS {
        builder = builder.tool(
            LocalToolName::parse(*tool).map_err(|_| invalid("tool"))?,
            *permission,
        );
    }
    for (uri, permission) in RESOURCES {
        builder = builder.resource(
            ResourceUri::new(*uri).map_err(|_| invalid("resource"))?,
            *permission,
        );
    }
    for (template, permission) in RESOURCE_TEMPLATES {
        builder = builder.resource_template(
            ResourceTemplateUri::new(*template).map_err(|_| invalid("resource template"))?,
            *permission,
        );
    }
    for prompt in TimePrompt::ALL {
        builder = builder.prompt(
            PromptName::parse(prompt.name()).map_err(|_| invalid("prompt"))?,
            PROMPTS,
        );
    }
    builder.build(&TimeContract::slug(), &TimeContract::scheme())
}

/// A request target whose required permission comes from the declaration.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Target<'a> {
    Tool(&'a str),
    Resource(&'a str),
    Prompt(&'a str),
}

/// The verified Hub identity, when it grants the permission `target` requires.
pub(crate) fn require(
    context: &RequestContext<RoleServer>,
    target: Target<'_>,
) -> Result<HubInternalIdentity, McpError> {
    let identity = hub_identity(context)?;
    authorize(&identity, target)?;
    Ok(identity)
}

/// The verified Hub identity, when it grants `permission` for a server-wide
/// operation that names no single target.
pub(crate) fn require_permission(
    context: &RequestContext<RoleServer>,
    permission: TimePermission,
) -> Result<HubInternalIdentity, McpError> {
    let identity = hub_identity(context)?;
    granted(&identity, &permission.name())?;
    Ok(identity)
}

/// Check the permission `target` requires against the Hub's grants. A target the
/// declaration omits is denied.
pub(crate) fn authorize(
    identity: &HubInternalIdentity,
    target: Target<'_>,
) -> Result<(), McpError> {
    granted(identity, &required(target)?)
}

pub(crate) fn required(target: Target<'_>) -> Result<PermissionName, McpError> {
    let undeclared = || {
        McpError::invalid_request(
            "This request names no target Time declares a permission for.".to_owned(),
            None,
        )
    };
    let declared = match target {
        Target::Tool(tool) => {
            let tool = LocalToolName::parse(tool).map_err(|_| undeclared())?;
            AUTHORIZATION
                .required(RequirementTarget::Tool(&tool))
                .cloned()
        }
        Target::Resource(uri) => {
            let uri = ResourceUri::new(uri).map_err(|_| undeclared())?;
            AUTHORIZATION
                .required(RequirementTarget::Resource(&uri))
                .cloned()
        }
        Target::Prompt(prompt) => {
            let prompt = PromptName::parse(prompt).map_err(|_| undeclared())?;
            AUTHORIZATION
                .required(RequirementTarget::Prompt(&prompt))
                .cloned()
        }
    };
    declared
        .map(|declared| declared.permission)
        .ok_or_else(undeclared)
}

fn granted(identity: &HubInternalIdentity, permission: &PermissionName) -> Result<(), McpError> {
    identity.grants(permission).then_some(()).ok_or_else(|| {
        McpError::invalid_request(
            format!(
                "You don't have permission to make this request. Missing permission `{permission}`."
            ),
            None,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_declaration_is_valid_and_declares_every_permission() {
        let declaration = declaration().unwrap();
        let declared: BTreeSet<_> = declaration
            .permissions
            .iter()
            .map(|permission| permission.permission.as_str().to_owned())
            .collect();
        let expected: BTreeSet<_> = TimePermission::ALL
            .iter()
            .map(|permission| permission.as_str().to_owned())
            .collect();
        assert_eq!(declared, expected);
    }

    #[test]
    fn every_tool_the_server_lists_has_exactly_one_requirement() {
        let listed: BTreeSet<String> = super::super::TimeMcp::tools()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        let required: BTreeSet<String> = TOOLS.iter().map(|(tool, _)| (*tool).to_owned()).collect();
        assert_eq!(listed, required);
        assert_eq!(TOOLS.len(), required.len(), "a tool is listed twice");
    }

    #[test]
    fn every_advertised_resource_and_template_resolves_to_a_requirement() {
        let setup = &*super::super::SERVER_SETUP;
        for resource in setup.resources() {
            let uri = resource.descriptor().uri.as_str();
            assert!(required(Target::Resource(uri)).is_ok(), "{uri}");
        }
        for (uri, _) in RESOURCES {
            assert!(required(Target::Resource(uri)).is_ok(), "{uri}");
        }
        for concrete in [
            "time://docs/agents",
            "time://zones/America/Argentina/Buenos_Aires",
            "time://zones/UTC",
            "time://calendars/ops/versions/3",
            "time://calendars?cursor=abc",
            "time://epochs/launch",
            "time://epochs/launch/versions/2",
            "time://epochs?cursor=abc",
            "time://events/event-1",
            "time://events?cursor=abc",
            "time://authorities/releases/tzdb-1",
            "time://authorities/releases?cursor=abc",
            "time://authorities/bootstrap/tzdb-1",
            "time://authorities/bootstrap?cursor=abc",
        ] {
            assert_eq!(
                required(Target::Resource(concrete)).unwrap(),
                AuthoritiesRead.name(),
                "{concrete}"
            );
        }
    }

    #[test]
    fn every_prompt_requires_reading_and_undeclared_targets_are_denied() {
        for prompt in TimePrompt::ALL {
            assert_eq!(
                required(Target::Prompt(prompt.name())).unwrap(),
                PROMPTS.name()
            );
        }
        assert!(required(Target::Tool("delete_everything")).is_err());
        assert!(required(Target::Resource("time://unknown")).is_err());
        assert!(required(Target::Resource("map://layers")).is_err());
        assert!(required(Target::Prompt("unknown_prompt")).is_err());
    }

    #[test]
    fn tool_annotations_agree_with_the_declaration() {
        let tools = super::super::TimeMcp::tools().list_all();
        veoveo_hub_sdk::registration::mcp::check_tool_annotations(&AUTHORIZATION, &tools).unwrap();
    }
}
