//! Existing hosted Time authority fixture shared by owner HTTP controls.
use super::*;
use veoveo_hub_sdk::hosting::{
    Hosted,
    testing::{self, TestHub},
};
pub(super) struct Fixture {
    pub hub: TestHub,
    pub state: Arc<crate::state::TimeApplication>,
    pub files: AuthorityFiles,
    pub db: TimeDb,
}
impl Fixture {
    pub async fn new() -> Self {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let state = files.replica(&db.a).await;
        let handler = state.clone();
        let hub = TestHub::new(
            testing::for_domain::<crate::mcp::TimeMcp>()
                .handler(move || {
                    Hosted::new(crate::mcp::TimeMcp::new(handler.clone())).with_tasks(
                        veoveo_hub_sdk::durable_tasks::DurableTasks::with_listener(
                            crate::server::tasks::TimeTaskExtension::new(handler.clone()),
                            crate::mcp::TimeSubscriptions::new(handler.clone()),
                        ),
                    )
                })
                .build(),
        )
        .granting(
            crate::contract::TimePermission::ALL
                .iter()
                .map(|permission| permission.name()),
        );
        Self {
            hub,
            state,
            files,
            db,
        }
    }
}
pub(super) fn token(tenant: &str) -> String {
    token_granting(tenant, crate::contract::TimePermission::ALL.iter().copied())
}

/// A forwarded access token for the test user in `tenant` that grants exactly
/// `permissions`, as the Hub's role evaluation would.
pub(super) fn token_granting(
    tenant: &str,
    permissions: impl IntoIterator<Item = crate::contract::TimePermission>,
) -> String {
    let mut principal = testing::principal();
    principal.tenant = Some(tenant.parse().unwrap());
    let mut authority = testing::authority();
    authority.tenant = tenant.parse().unwrap();
    let trust = testing::access_token_trust();
    veoveo_hub_sdk::HubInternalTokenIssuer::new(trust.issuer.clone(), testing::signing_key())
        .unwrap()
        .issue(veoveo_hub_sdk::ForwardedRequest {
            deployment: trust.deployment,
            delegation_context: None,
            profile: "operations".parse().unwrap(),
            server: "time".parse().unwrap(),
            audience: trust.resource.clone(),
            client: testing::TEST_CLIENT.parse().unwrap(),
            actor: principal,
            authority,
            permissions: permissions
                .into_iter()
                .map(|permission| permission.name())
                .collect(),
            request_context: None,
            acting_server: None,
            acting_task: None,
            expires_at: Utc::now() + chrono::TimeDelta::minutes(5),
        })
        .unwrap()
        .bearer_token
}
