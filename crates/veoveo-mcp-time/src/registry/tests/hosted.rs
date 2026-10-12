//! Existing hosted Time authority fixture shared by owner HTTP controls.
use super::*;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};
use veoveo_platform_store::Instance;
pub(super) struct Fixture {
    pub gateway: TestGateway,
    pub state: Arc<crate::state::TimeApplication>,
    pub files: AuthorityFiles,
    pub db: TimeDb,
}
impl Fixture {
    pub async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.a.pool().clone());
        let acquisitions = crate::acquisition::AcquisitionService::new(
            crate::acquisition::AcquisitionServiceConfig {
                scratch_root: files.root.path().join("scratch"),
                release_root: files.root.path().join("releases"),
                zic_executable: files.root.path().join("unavailable-zic"),
                maximum_source_bytes: 1024,
                maximum_expanded_bytes: 4096,
                timeout: Duration::from_secs(1),
            },
            catalog.clone(),
        )
        .unwrap();
        let instance = Instance::start(&db.a, &crate::test_database::slug())
            .await
            .unwrap();
        let state = Arc::new(crate::state::TimeApplication {
            store: db.a.clone(),
            tasks: veoveo_task_runtime::TaskRuntime::new(
                db.a.clone(),
                crate::test_database::slug(),
                Arc::new(instance),
            ),
            catalog,
            authorities: files.registry(),
            clock: crate::clock::ClockMonitor::new(
                crate::clock::ClockSource::System,
                Duration::from_secs(1),
            ),
            acquisitions: Arc::new(acquisitions),
            subscriptions: Arc::new(crate::changes::TenantSubscriptions::default()),
        });
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<crate::mcp::TimeMcp>()
                .handler(move || {
                    Hosted::new(crate::mcp::TimeMcp::new(handler.clone())).with_tasks(
                        veoveo_task_runtime::DurableTasks::with_listener(
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
    veoveo_mcp_contract::GatewayInternalTokenIssuer::new(
        trust.issuer.clone(),
        testing::signing_key(),
    )
    .unwrap()
    .issue(veoveo_mcp_contract::ForwardedRequest {
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
