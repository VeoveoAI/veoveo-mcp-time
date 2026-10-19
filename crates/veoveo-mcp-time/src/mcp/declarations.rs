//! Time's registration declaration. Time registers it with the Hub and advertises the
//! same value in MCP Discover, so both have one source.
use std::sync::OnceLock;

use veoveo_hub_sdk::registration::{RegistrationDeclaration, ServerEndpoint, ServerVersion};
use veoveo_hub_sdk::{PublicDeployment, server_contract::McpServerContract};

use super::setup::TimeContract;

/// The registration Time advertises. Startup sets it from the deployment before the
/// MCP setup is built; until then Discover advertises no declarations.
pub(crate) static REGISTRATION: OnceLock<RegistrationDeclaration> = OnceLock::new();

/// Time's identity and the URL where the Hub reaches its MCP endpoint:
/// `{public_base_url}/time/mcp`.
pub(crate) fn registration(
    deployment: &PublicDeployment,
) -> anyhow::Result<RegistrationDeclaration> {
    let endpoint = deployment.server(crate::changes::server().as_str())?;
    let url = url::Url::parse(&format!("{}/mcp", endpoint.public_url()))?;
    Ok(RegistrationDeclaration {
        slug: crate::changes::server(),
        uri_scheme: TimeContract::scheme(),
        version: ServerVersion::new(env!("CARGO_PKG_VERSION"))?,
        endpoint: ServerEndpoint::new(url)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registered_endpoint_is_the_mounted_mcp_url() {
        let deployment = PublicDeployment::new("http://vv-mcp-time.internal:8800").unwrap();
        let registration = registration(&deployment).unwrap();
        assert_eq!(
            registration.endpoint.url().as_str(),
            "http://vv-mcp-time.internal:8800/time/mcp"
        );
        assert_eq!(registration.slug.as_str(), "time");
        assert_eq!(registration.uri_scheme.as_str(), "time");
    }
}
