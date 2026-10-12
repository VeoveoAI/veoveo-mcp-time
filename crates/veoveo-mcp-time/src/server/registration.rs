//! Registration with the Gateway: Time's key, its declarations, and the client that
//! renews the lease while Time serves.
use anyhow::{Context, Result};
use veoveo_mcp_ext_registration::{
    ClientMetadata, RegistrationDeclaration,
    client::{KeyDirectory, Registration, RegistrationClient},
};

use super::config::Args;
use crate::mcp::requirements::AUTHORIZATION;

/// Register Time and return the client that maintains the registration.
pub(super) async fn register(
    args: &Args,
    declaration: RegistrationDeclaration,
) -> Result<(RegistrationClient, Registration)> {
    let key = KeyDirectory::new(&args.key_directory)
        .load_or_generate()
        .context("loading Time's registration key")?;
    let metadata = ClientMetadata::new(*key.public_key(), declaration, AUTHORIZATION.clone())?;
    let http = reqwest::Client::builder()
        .build()
        .context("building the registration HTTP client")?;
    let endpoint = RegistrationClient::discover(&http, &args.gateway_issuer)
        .await
        .context("discovering the Gateway's registration endpoint")?;
    let client = RegistrationClient::new(
        http,
        endpoint,
        args.registration_secret.clone(),
        key,
        metadata,
    )?;
    let registration = client
        .register()
        .await
        .context("registering Time with the Gateway")?;
    tracing::info!(client_id = %registration.client_id(), "registered with the Gateway");
    Ok((client, registration))
}
