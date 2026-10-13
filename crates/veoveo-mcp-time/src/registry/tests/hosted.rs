//! Existing hosted Time authority fixture shared by owner HTTP controls.
use super::*;
use veoveo_sdk_rust::hosting::{
    Hosted,
    testing::{self, TestGateway},
};
pub(super) struct Fixture {
    pub gateway: TestGateway,
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
        let gateway = TestGateway::new(
            testing::for_domain::<crate::mcp::TimeMcp>()
                .handler(move || {
                    Hosted::new(crate::mcp::TimeMcp::new(handler.clone())).with_tasks(
                        veoveo_sdk_rust::durable_tasks::DurableTasks::with_listener(
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
            gateway,
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
/// `permissions`, as the Gateway's role evaluation would.
pub(super) fn token_granting(
    tenant: &str,
    permissions: impl IntoIterator<Item = crate::contract::TimePermission>,
) -> String {
    let mut principal = testing::principal();
    principal.tenant = Some(tenant.parse().unwrap());
    let mut authority = testing::authority();
    authority.tenant = tenant.parse().unwrap();
    let trust = testing::access_token_trust();
    veoveo_sdk_rust::GatewayInternalTokenIssuer::new(trust.issuer.clone(), testing::signing_key())
        .unwrap()
        .issue(veoveo_sdk_rust::ForwardedRequest {
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
            expires_at: Utc::now() + chrono::TimeDelta::minutes(5),
        })
        .unwrap()
        .bearer_token
}
