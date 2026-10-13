mod calculation;
mod settlement;
#[cfg(test)]
mod tests;

use crate::contract::TimeTaskKind;
use std::{collections::BTreeSet, sync::Arc};
use veoveo_types::TaskTypeDefinition;

use futures::StreamExt;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskFailure, TaskOwner, TaskRetentionPin, TaskRuntime, TaskSnapshot,
    TaskTransition,
};
use veoveo_types::TaskId;

use crate::{
    contract::{ExpandScheduleRequest, ValidateTimelineRequest},
    mcp::requirements::{Target, authorize},
    state::TimeApplication,
};

const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 1_000;

#[derive(Clone)]
pub(crate) struct TimeTaskExtension {
    state: Arc<TimeApplication>,
}

#[derive(Clone)]
pub(crate) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum TimeTaskRequest {
    ExpandSchedule(Box<ExpandScheduleRequest>),
    ValidateTimeline(ValidateTimelineRequest),
}

impl TimeTaskExtension {
    pub(crate) fn new(state: Arc<TimeApplication>) -> Self {
        Self { state }
    }
}

impl veoveo_task_runtime::DurableTaskService for TimeTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        context: &rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<Self::Caller, rmcp::ErrorData> {
        let identity = veoveo_mcp_contract::hosting::gateway_identity(context)?;
        Ok(AuthenticatedCaller { identity })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<Option<rmcp::model::CreateTaskResult>, rmcp::ErrorData> {
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let task =
            match TimeTaskKind::from_wire_name(request.name.as_ref()) {
                Some(TimeTaskKind::ExpandSchedule) => {
                    authorize(&caller.identity, Target::Tool(request.name.as_ref()))?;
                    TimeTaskRequest::ExpandSchedule(Box::new(
                        serde_json::from_value(arguments).map_err(|error| {
                            rmcp::ErrorData::invalid_params(error.to_string(), None)
                        })?,
                    ))
                }
                Some(TimeTaskKind::ValidateTimeline) => {
                    authorize(&caller.identity, Target::Tool(request.name.as_ref()))?;
                    TimeTaskRequest::ValidateTimeline(serde_json::from_value(arguments).map_err(
                        |error| rmcp::ErrorData::invalid_params(error.to_string(), None),
                    )?)
                }
                None => return Ok(None),
            };
        let retention_pins = veoveo_task_runtime::retention_pins(request.meta.as_ref())?;
        let snapshot = create_time_task(&self.state, &caller.identity, task, retention_pins)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(Some(rmcp::model::CreateTaskResult::new(
            veoveo_task_runtime::task_seed(&snapshot),
        )))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::GetTaskParams,
    ) -> Result<rmcp::model::GetTaskResult, rmcp::ErrorData> {
        veoveo_task_runtime::get_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::UpdateTaskParams,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_task_runtime::update_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        task_id: String,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_task_runtime::cancel_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            task_id,
        )
        .await
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<veoveo_task_runtime::DurableTaskSubscription, rmcp::ErrorData> {
        veoveo_task_runtime::subscribe_durable_tasks(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            task_ids,
        )
        .await
    }
}

/// Queue a Task. A replica's work stream claims and runs it.
async fn create_time_task(
    state: &TimeApplication,
    identity: &GatewayInternalIdentity,
    request: TimeTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> anyhow::Result<TaskSnapshot> {
    let task = time_task(
        &state.tasks,
        runtime_owner(identity),
        &request,
        retention_pins,
    )?;
    Ok(state.tasks.create(task).await?.snapshot)
}

/// A Time calculation as a durable Task. Its deterministic output lets a replica resume
/// it after the instance that claimed it dies.
fn time_task(
    tasks: &TaskRuntime,
    owner: TaskOwner,
    request: &TimeTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> serde_json::Result<CreateTask> {
    Ok(CreateTask {
        task_id: TaskId::new(),
        owner,
        server: tasks.server().clone(),
        task_type: request.task_type(),
        request: serde_json::to_value(request)?,
        recovery_class: RecoveryClass::Resume,
        idempotency_key: None,
        ttl_ms: Some(TASK_TTL_MS),
        poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
        retention_pins,
    })
}

/// Run every Task this process's instance claims until `stop` is cancelled. A Task
/// whose instance died returns to the queue, so a replica claims and resumes it here.
/// The claim stream fails when the instance loses its lock, and so does this consumer.
pub(super) async fn work(
    state: Arc<TimeApplication>,
    stop: CancellationToken,
) -> anyhow::Result<()> {
    let mut claimed = state.tasks.work().await?;
    loop {
        let next = tokio::select! {
            () = stop.cancelled() => return Ok(()),
            next = claimed.next() => next,
        };
        let snapshot = next.ok_or_else(|| anyhow::anyhow!("the Task claim stream ended"))??;
        if let Err(error) = execute(state.clone(), snapshot, stop.child_token()).await {
            tracing::warn!(%error, "Time could not start a claimed Task");
        }
        state.tasks.reap_workers().await;
    }
}

/// Start the worker of one claimed Task. A request this version cannot read fails the
/// Task instead of leaving it claimed.
async fn execute(
    state: Arc<TimeApplication>,
    snapshot: TaskSnapshot,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let task_id = snapshot.task_id;
    let request = serde_json::from_value::<TimeTaskRequest>(snapshot.request.clone())
        .ok()
        .filter(|request| {
            TimeTaskKind::from_name(&snapshot.task_type).is_some()
                && request.task_type() == snapshot.task_type
        });
    let Some(request) = request else {
        settlement::update(
            &state.tasks,
            task_id,
            TaskTransition::Failed(TaskFailure::new(
                "unsupported_time_task",
                "the Task type or request is not a Time calculation",
            )),
            &cancellation,
        )
        .await?;
        return Ok(());
    };
    let join = tokio::spawn(run_time_task(
        state.clone(),
        task_id,
        snapshot.owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(task_id, cancellation, join)
        .await?;
    Ok(())
}

async fn run_time_task(
    state: Arc<TimeApplication>,
    task_id: TaskId,
    owner: TaskOwner,
    request: TimeTaskRequest,
    cancellation: CancellationToken,
) {
    let mut calculation = calculation::Calculation::new(&cancellation);
    if let Err(error) = run_time_task_inner(
        state,
        task_id,
        owner,
        request,
        cancellation,
        &mut calculation,
    )
    .await
    {
        tracing::warn!(%task_id, "Time task execution did not settle: {error}");
    }
    // Cooperative stop and join complete before this registered worker exits.
    calculation.stop_and_drain().await;
}

async fn run_time_task_inner(
    state: Arc<TimeApplication>,
    task_id: TaskId,
    owner: TaskOwner,
    request: TimeTaskRequest,
    cancellation: CancellationToken,
    calculation: &mut calculation::Calculation,
) -> anyhow::Result<()> {
    update_task(
        &state,
        task_id,
        TaskTransition::Running {
            message: request.description().to_owned(),
            progress: 0.05,
        },
        &cancellation,
    )
    .await?;
    if !settlement::continue_work(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    let scope = match state.scope_from_task_owner(&owner).await {
        Ok(scope) => scope,
        Err(error) => {
            return fail_task(
                &state,
                task_id,
                "temporal_calculation_failed",
                error,
                &cancellation,
            )
            .await;
        }
    };
    if !settlement::continue_work(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    let engine = match &request {
        TimeTaskRequest::ExpandSchedule(_) => state.engine(&scope).await,
        TimeTaskRequest::ValidateTimeline(request) => {
            state
                .engine_for_expressions(&scope, request.points.iter().map(|point| &point.at))
                .await
        }
    };
    let engine = match engine {
        Ok(engine) => engine,
        Err(error) => {
            return fail_task(
                &state,
                task_id,
                "temporal_calculation_failed",
                error,
                &cancellation,
            )
            .await;
        }
    };
    if !settlement::continue_work(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    let result = calculation
        .run(move |stop| match request {
            TimeTaskRequest::ExpandSchedule(request) => engine
                .expand_schedule_with_stop(&request, &stop)
                .and_then(|output| tool_result("expanded operational schedule", &output)),
            TimeTaskRequest::ValidateTimeline(request) => engine
                .validate_timeline_with_stop(&request, &stop)
                .and_then(|output| tool_result("validated mission timeline", &output)),
        })
        .await;
    if result
        .as_ref()
        .is_err_and(|error| error.is::<crate::engine::CalculationStopped>())
    {
        // A cooperative stop is not a calculation failure. Observe durable intent
        // to settle remote/local cancellation, or leave shutdown for Resume.
        settlement::continue_work(&state.tasks, task_id, &cancellation).await?;
        return Ok(());
    }
    // Shutdown can cancel a local worker without a durable cancellation request.
    // Stop publication and leave the Task to its instance's recovery.
    if settlement::local_stop(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    match result {
        Ok(result) => {
            match veoveo_task_runtime::mcp_task_completion("Temporal calculation completed", result)
            {
                Ok(transition) => update_task(&state, task_id, transition, &cancellation).await,
                Err(error) => {
                    fail_task(
                        &state,
                        task_id,
                        "result_serialization_failed",
                        error,
                        &cancellation,
                    )
                    .await
                }
            }
        }
        Err(error) => {
            fail_task(
                &state,
                task_id,
                "temporal_calculation_failed",
                error,
                &cancellation,
            )
            .await
        }
    }
}

impl TimeTaskRequest {
    fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::ExpandSchedule(_) => TimeTaskKind::ExpandSchedule.name(),
            Self::ValidateTimeline(_) => TimeTaskKind::ValidateTimeline.name(),
        }
    }
    fn description(&self) -> &'static str {
        match self {
            Self::ExpandSchedule(_) => "expanding operational calendar",
            Self::ValidateTimeline(_) => "validating mission timeline",
        }
    }
}

fn tool_result<T: Serialize>(text: &str, value: &T) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value)?);
    Ok(result)
}

async fn fail_task(
    state: &TimeApplication,
    task_id: TaskId,
    code: &str,
    error: impl std::fmt::Display,
    cancellation: &CancellationToken,
) -> anyhow::Result<()> {
    update_task(
        state,
        task_id,
        TaskTransition::Failed(TaskFailure::new(code, error.to_string())),
        cancellation,
    )
    .await
}
async fn update_task(
    state: &TimeApplication,
    task_id: TaskId,
    transition: TaskTransition,
    cancellation: &CancellationToken,
) -> anyhow::Result<()> {
    settlement::update(&state.tasks, task_id, transition, cancellation).await?;
    Ok(())
}

fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.actor.id.clone(),
        principal_kind: match identity.actor.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
        },
        issuer: identity.actor.issuer.clone(),
        subject: identity.actor.subject.clone(),
        profile: identity.profile.clone(),
        tenant_key: identity.actor.tenant.clone(),
        data_labels: identity.actor.data_labels.clone(),
        authority: identity.authority.clone(),
    }
}
