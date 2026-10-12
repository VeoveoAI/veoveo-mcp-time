mod bootstrap;
mod config;
mod registration;
pub(crate) mod tasks;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    PublicDeployment, TelemetryGuard,
    hosting::{Hosted, HostedServer},
    init_server_telemetry,
};
use veoveo_mcp_ext_registration::client::Registration;
use veoveo_task_runtime::{DurableTasks, TaskRuntime, TaskRuntimeConfig};

use crate::{
    acquisition::{AcquisitionService, AcquisitionServiceConfig},
    authority::{AuthorityContext, LeapSecondTable},
    catalog::TimeCatalog,
    changes::{self, ChangeWatcher, TenantSubscriptions},
    clock::{ClockMonitor, ClockSource},
    contract::{AuthorityDatasetKind, EffectiveTimeAuthority},
    due::run_due_events,
    mcp::{TimeMcp, TimeSubscriptions, declarations},
    registry::AuthorityRegistry,
    state::TimeApplication,
};

use config::Args;
use tasks::TimeTaskExtension;

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
    // The registration lasts while Time's login holds a live process instance. Once
    // serving ends, on SIGTERM or Ctrl-C or on a failure, the client deletes it, which
    // also drops the login. Time never renews it.
    let issued = registration.clone();
    let mut served = None;
    let maintained = registration_client
        .maintain(registration, async {
            served = Some(serve(&args, &public_deployment, &issued).await);
        })
        .await;
    if let Err(error) = maintained {
        tracing::warn!(%error, "Time could not remove its registration");
    }
    served.context("Time's registration ended before serving finished")?
}

/// Connect with the registration's database login, migrate Time's schema and serve until
/// SIGTERM or Ctrl-C, or until the process instance loses its lock.
async fn serve(
    args: &Args,
    public_deployment: &PublicDeployment,
    registration: &Registration,
) -> Result<()> {
    let grant = registration
        .database()
        .context("the Gateway issued no database login for Time")?;
    // The runtime opens the pool for the registration's login and starts this process's
    // instance, which owns the Tasks it claims.
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            grant.url.as_str(),
            grant.username.as_str(),
            grant.password.clone(),
        )?,
        changes::server(),
    )
    .await
    .context("connecting to the installation database")?;
    let store = tasks.platform_store().clone();
    store
        .migrate_server(&changes::server(), crate::persistence::migrator())
        .await
        .context("applying Time's migrations")?;
    let instance = tasks.instance().clone();
    let catalog = TimeCatalog::new(store.pool().clone());
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
        store: store.clone(),
        tasks,
        catalog: catalog.clone(),
        authorities: authorities.clone(),
        clock,
        acquisitions,
        subscriptions: Arc::new(TenantSubscriptions::default()),
    });

    let stop = CancellationToken::new();
    let wake_due = Arc::new(Notify::new());
    let watching = tokio::spawn(
        ChangeWatcher {
            subscriptions: state.subscriptions.clone(),
            authorities,
            due: wake_due.clone(),
        }
        .run(store.listen(vec![changes::channel()])),
    );
    let due = tokio::spawn(run_due_events(catalog, wake_due, stop.clone()));
    let mut working = tokio::spawn(tasks::work(state.clone(), stop.clone()));

    let clock_state = state.clone();
    let handler_state = state.clone();
    let hosted = HostedServer::for_domain::<TimeMcp>()
        .deployment(public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(registration.access_tokens())?
        .handler(move || {
            Hosted::new(TimeMcp::new(handler_state.clone())).with_tasks(
                DurableTasks::with_listener(
                    TimeTaskExtension::new(handler_state.clone()),
                    TimeSubscriptions::new(handler_state.clone()),
                ),
            )
        })
        // Time serves only while it observes a clock.
        .readiness(move || {
            let state = clock_state.clone();
            async move { state.clock.quality().await.is_ok() }
        })
        .build();
    // Serving ends on SIGTERM or Ctrl-C, or when Time can no longer claim Tasks, which
    // includes the loss of its instance lock.
    let served = tokio::select! {
        served = hosted.serve(SocketAddr::from(([0, 0, 0, 0], args.port))) => served,
        worked = &mut working => match worked {
            Ok(Ok(())) => Err(anyhow::anyhow!("Time's Task consumer stopped")),
            Ok(Err(error)) => Err(error.context("Time stopped claiming Tasks")),
            Err(error) => Err(anyhow::Error::from(error).context("Time's Task consumer failed")),
        },
    };
    stop.cancel();
    watching.abort();
    let _ = due.await;
    if !working.is_finished() {
        let _ = working.await;
    }
    drop(state);
    // Deleting the instance's row releases the Tasks it still owns at once.
    if let Err(error) = instance.shutdown().await {
        tracing::warn!(%error, "Time's process instance did not stop cleanly");
    }
    store.close().await;
    served
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
