mod bootstrap;
mod config;
mod registration;
pub(crate) mod tasks;

use std::{collections::BTreeMap, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use veoveo_mcp_contract::{
    SubscriptionHub, TelemetryGuard,
    hosting::{Hosted, HostedServer},
    init_server_telemetry,
};
use veoveo_task_runtime::{
    DurableTasks, StoreAuthLevel, TaskRecoveryObserver, TaskRuntime, TaskRuntimeConfig,
};

use crate::{
    acquisition::{AcquisitionService, AcquisitionServiceConfig},
    admin,
    authority::{AuthorityContext, LeapSecondTable},
    catalog::TimeCatalog,
    clock::{ClockMonitor, ClockSource},
    contract::{AuthorityDatasetKind, EffectiveTimeAuthority},
    mcp::{TimeMcp, TimeSubscriptions, declarations},
    registry::AuthorityRegistry,
    state::TimeApplication,
};

use config::Args;
use tasks::{TimeTaskExtension, recover_tasks};

const SERVER_SLUG: &str = "time";

pub async fn run() -> Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-mcp-time", "info,veoveo_mcp_time=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let declaration = declarations::registration(&public_deployment)?;
    declarations::REGISTRATION
        .set(declaration.clone())
        .map_err(|_| anyhow::anyhow!("Time's registration was declared twice"))?;
    std::sync::LazyLock::force(&crate::mcp::SERVER_SETUP);
    let (registration_client, registration) = registration::register(&args, declaration).await?;
    let database = registration
        .database()
        .context("the Gateway issued no database user for Time's module")?;
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            database.endpoint.as_str().trim_end_matches('/'),
            database.namespace.as_str(),
            database.database.as_str(),
            StoreAuthLevel::Database,
            database.username.as_str(),
            database.password.clone(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let recovery = tasks.observe_startup_recovery().await?;
    let catalog = TimeCatalog::new(tasks.platform_store().clone());
    let leap_seconds = LeapSecondTable::from_path(&args.bootstrap_leap_seconds_file).await?;
    let bootstrap = AuthorityContext::from_paths(
        EffectiveTimeAuthority::new(
            bootstrap::reference(AuthorityDatasetKind::Tzdb, &args.bootstrap_tzdb_source_file)
                .await?,
            bootstrap::reference(
                AuthorityDatasetKind::LeapSeconds,
                &args.bootstrap_leap_seconds_file,
            )
            .await?,
        )?,
        &args.bootstrap_tzdb_directory,
        leap_seconds,
    )?;
    let authorities = AuthorityRegistry::new(
        bootstrap,
        args.bootstrap_tzdb_directory.clone(),
        args.bootstrap_leap_seconds_file.clone(),
    );
    let clock = ClockMonitor::new(
        args.ntpd_observation_socket
            .clone()
            .map_or(ClockSource::System, |observation_socket| {
                ClockSource::NtpdRs { observation_socket }
            }),
        Duration::from_secs(args.clock_observation_timeout_seconds),
    );
    let acquisitions = Arc::new(AcquisitionService::new(
        AcquisitionServiceConfig {
            scratch_root: args.acquisition_scratch_directory.clone(),
            release_root: args.release_directory.clone(),
            zic_executable: args.zic_executable.clone(),
            maximum_source_bytes: args.maximum_source_bytes,
            maximum_expanded_bytes: args.maximum_expanded_bytes,
            timeout: Duration::from_secs(args.acquisition_timeout_seconds),
        },
        catalog.clone(),
    )?);
    let state = Arc::new(TimeApplication {
        tasks,
        catalog,
        authorities,
        clock,
        acquisitions,
        subscriptions: Arc::new(SubscriptionHub::new()),
        event_watchers: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
    });
    let recovery_state = state.clone();
    let recovery_observer = TaskRecoveryObserver::start(recovery, move |report| {
        let state = recovery_state.clone();
        async move { recover_tasks(state, report.resumable).await }
    })
    .await?;

    let resource_state = state.clone();
    let _resource_observer = tokio::spawn(async move {
        use crate::TimeObservationTable::*;
        use futures::StreamExt;
        let mut changes = resource_state.tasks.platform_store().resource_changes(vec![
            TimeSource,
            TimeAuthorityRelease,
            TimeActiveAuthority,
            TimeAcquisition,
            TimeCalendarVersion,
            TimeEpoch,
            TimeEvent,
            TimeClockPolicy,
        ]);
        while changes.next().await.is_some() {
            resource_state.authorities.invalidate().await;
            resource_state
                .subscriptions
                .notify_resource_contents_changed()
                .await;
        }
    });
    let admin = admin::router(state.clone());
    let clock_state = state.clone();
    let server = HostedServer::for_domain::<TimeMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(registration.access_tokens())?
        .handler(move || {
            Hosted::new(TimeMcp::new(state.clone())).with_tasks(DurableTasks::with_resources(
                TimeTaskExtension::new(state.clone()),
                TimeSubscriptions::new(state.clone()),
            ))
        })
        .admin_routes(admin)
        // Time serves only while it observes a clock.
        .readiness(move || {
            let state = clock_state.clone();
            async move { state.clock.quality().await.is_ok() }
        })
        .build();
    let (stop_registration, stopped) = tokio::sync::oneshot::channel::<()>();
    let maintenance = tokio::spawn(async move {
        registration_client
            .maintain(registration, async {
                let _ = stopped.await;
            })
            .await
    });
    let served = recovery_observer
        .serve(server.serve(SocketAddr::from(([0, 0, 0, 0], args.port))))
        .await;
    let _ = stop_registration.send(());
    match maintenance.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(%error, "Time's registration ended with an error"),
        Err(error) => tracing::warn!(%error, "Time's registration task failed"),
    }
    served
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
