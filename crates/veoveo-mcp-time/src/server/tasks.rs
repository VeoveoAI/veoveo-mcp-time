mod calculation;
#[cfg(test)]
mod tests;

use crate::contract::TimeTaskKind;
use std::{collections::BTreeSet, sync::Arc};
use veoveo_types::TaskTypeDefinition;

use rmcp::model::{CallToolResult, ContentBlock};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_hub_sdk::{
    HubInternalIdentity, PrincipalKind,
    durable_tasks::{TaskContext, TaskWorker},
};
use veoveo_task::{
    CreateTask, RecoveryClass, TaskFailure, TaskOwner, TaskRetentionPin, TaskRuntime, TaskSnapshot,
    TaskSurface, TaskTransition,
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
    identity: HubInternalIdentity,
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

impl veoveo_hub_sdk::durable_tasks::DurableTaskService for TimeTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        context: &rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<Self::Caller, rmcp::ErrorData> {
        let identity = veoveo_hub_sdk::hosting::hub_identity(context)?;
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
        let retention_pins = veoveo_hub_sdk::durable_tasks::retention_pins(request.meta.as_ref())?;
        let snapshot = create_time_task(&self.state, &caller.identity, task, retention_pins)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        Ok(Some(rmcp::model::CreateTaskResult::new(
            veoveo_hub_sdk::durable_tasks::task_seed(&snapshot),
        )))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::GetTaskParams,
    ) -> Result<rmcp::model::GetTaskResult, rmcp::ErrorData> {
        veoveo_hub_sdk::durable_tasks::get_durable_task(
            &self.state.tasks,
            &self
                .state
                .tasks
                .for_owner(&runtime_owner(&caller.identity)?),
            request,
        )
        .await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::UpdateTaskParams,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_hub_sdk::durable_tasks::update_durable_task(
            &self
                .state
                .tasks
                .for_owner(&runtime_owner(&caller.identity)?),
            request,
        )
        .await
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        task_id: String,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_hub_sdk::durable_tasks::cancel_durable_task(
            &self
                .state
                .tasks
                .for_owner(&runtime_owner(&caller.identity)?),
            task_id,
        )
        .await
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<veoveo_hub_sdk::durable_tasks::DurableTaskSubscription, rmcp::ErrorData> {
        veoveo_hub_sdk::durable_tasks::subscribe_durable_tasks(
            &self.state.tasks,
            &self
                .state
                .tasks
                .for_owner(&runtime_owner(&caller.identity)?),
            task_ids,
        )
        .await
    }
}

/// Queue a Task. A replica's work stream claims and runs it.
async fn create_time_task(
    state: &TimeApplication,
    identity: &HubInternalIdentity,
    request: TimeTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> anyhow::Result<TaskSnapshot> {
    let task = time_task(
        &state.tasks,
        runtime_owner(identity)?,
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
/// Once stopped, the worker waits for its running Tasks: a calculation interrupted by
/// the stop publishes nothing and stays with its instance, whose recovery queues it.
pub(super) async fn work(
    state: Arc<TimeApplication>,
    stop: CancellationToken,
) -> anyhow::Result<()> {
    TaskWorker::new(state.tasks.clone())
        .run(
            move |ctx| {
                let state = state.clone();
                async move { run_time_task(state, ctx).await }
            },
            stop,
        )
        .await?;
    Ok(())
}

/// One claimed Time Task. A request this version cannot read fails the Task.
async fn run_time_task(
    state: Arc<TimeApplication>,
    ctx: TaskContext,
) -> anyhow::Result<TaskTransition> {
    let snapshot = ctx.task();
    let request = serde_json::from_value::<TimeTaskRequest>(snapshot.request.clone())
        .ok()
        .filter(|request| {
            TimeTaskKind::from_name(&snapshot.task_type).is_some()
                && request.task_type() == snapshot.task_type
        });
    let Some(request) = request else {
        return Ok(failed(
            "unsupported_time_task",
            "the Task type or request is not a Time calculation",
        ));
    };
    let mut calculation = calculation::Calculation::new(&ctx.cancellation_token());
    let outcome = calculate(&state, &ctx, request, &mut calculation).await;
    // Cooperative stop and join complete before this worker settles or releases.
    calculation.stop_and_drain().await;
    outcome
}

/// The calculation of one Time Task. It checks for a stop or a cancellation between its
/// steps, and the worker settles a requested cancellation or leaves a stopping Task to
/// its recovery.
async fn calculate(
    state: &TimeApplication,
    ctx: &TaskContext,
    request: TimeTaskRequest,
    calculation: &mut calculation::Calculation,
) -> anyhow::Result<TaskTransition> {
    ctx.update(TaskTransition::Running {
        message: request.description().to_owned(),
        progress: 0.05,
    })
    .await?;
    ctx.checkpoint().await?;
    let scope = match state.scope_from_task_owner(&ctx.task().owner).await {
        Ok(scope) => scope,
        Err(error) => return Ok(failed("temporal_calculation_failed", error)),
    };
    ctx.checkpoint().await?;
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
        Err(error) => return Ok(failed("temporal_calculation_failed", error)),
    };
    ctx.checkpoint().await?;
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
    match result {
        Ok(result) => Ok(
            match veoveo_hub_sdk::durable_tasks::mcp_task_completion(
                "Temporal calculation completed",
                result,
            ) {
                Ok(transition) => transition,
                Err(error) => failed("result_serialization_failed", error),
            },
        ),
        // A cooperative stop is not a calculation failure: the stop or the
        // cancellation that caused it ends the work.
        Err(error) if error.is::<crate::engine::CalculationStopped>() => {
            ctx.checkpoint().await?;
            Err(error)
        }
        Err(error) => Ok(failed("temporal_calculation_failed", error)),
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

fn failed(code: &str, error: impl std::fmt::Display) -> TaskTransition {
    TaskTransition::Failed(TaskFailure::new(code, error.to_string()))
}

/// The caller as a Task owner. A Task's surface is the Hub profile the call arrived
/// on, so the caller reads it back only through that profile.
fn runtime_owner(identity: &HubInternalIdentity) -> Result<TaskOwner, rmcp::ErrorData> {
    let surface = TaskSurface::parse(identity.profile.as_str()).map_err(|_| {
        rmcp::ErrorData::invalid_request("the Hub profile is not a Task surface", None)
    })?;
    Ok(TaskOwner {
        principal_key: identity.actor.id.clone(),
        principal_kind: match identity.actor.kind {
            PrincipalKind::User => veoveo_task::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task::PrincipalKind::Service,
        },
        issuer: identity.actor.issuer.clone(),
        subject: identity.actor.subject.clone(),
        surface,
        tenant_key: identity.actor.tenant.clone(),
        data_labels: identity.actor.data_labels.clone(),
        authority: identity.authority.clone(),
    })
}
