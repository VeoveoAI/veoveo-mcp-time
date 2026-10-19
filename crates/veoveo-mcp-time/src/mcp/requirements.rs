//! Operational admission and registration derive from the ready typed routes.
use super::setup::TimeContract;
use crate::TimePermission;
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use std::sync::LazyLock;
use veoveo_hub_sdk::{
    HubInternalIdentity, LocalToolName,
    hosting::hub_identity,
    registration::{AuthorizationDeclaration, RequirementTarget, RouteAccess},
    server_contract::McpServerContract,
};
use veoveo_types::ResourceUri;

pub(crate) const SERVER: TimePermission = TimePermission::AuthoritiesRead;
pub(crate) static AUTHORIZATION: LazyLock<AuthorizationDeclaration> = LazyLock::new(|| {
    let declaration = super::ROUTES.authorization();
    declaration
        .check(&crate::changes::server(), &TimeContract::scheme())
        .expect("Time route authorization");
    declaration
});
#[derive(Debug, Clone, Copy)]
pub(crate) enum Target<'a> {
    Tool(&'a str),
    Resource(&'a str),
}
pub(crate) fn require_permission(
    context: &RequestContext<RoleServer>,
    permission: TimePermission,
) -> Result<HubInternalIdentity, McpError> {
    let identity = hub_identity(context)?;
    granted(&identity, &permission.name())?;
    Ok(identity)
}
pub(crate) fn authorize(
    identity: &HubInternalIdentity,
    target: Target<'_>,
) -> Result<(), McpError> {
    let undeclared = || McpError::invalid_request("the target is not a declared Time route", None);
    let access = match target {
        Target::Tool(name) => AUTHORIZATION
            .access(RequirementTarget::Tool(
                &LocalToolName::parse(name).map_err(|_| undeclared())?,
            ))
            .cloned(),
        Target::Resource(uri) => AUTHORIZATION
            .access(RequirementTarget::Resource(
                &ResourceUri::new(uri).map_err(|_| undeclared())?,
            ))
            .cloned(),
    }
    .ok_or_else(undeclared)?;
    match access {
        RouteAccess::Authenticated => Ok(()),
        RouteAccess::Permission(permission) => granted(identity, &permission),
    }
}
fn granted(
    identity: &HubInternalIdentity,
    permission: &veoveo_types::PermissionName,
) -> Result<(), McpError> {
    identity.grants(permission).then_some(()).ok_or_else(|| {
        McpError::invalid_request(
            format!("required route permission `{permission}` was not granted"),
            None,
        )
    })
}
