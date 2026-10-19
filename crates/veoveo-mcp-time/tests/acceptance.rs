//! Acceptance through a running local installation: Time registered with the Hub and
//! called through the Hub by the installation's member and admin callers.
//!
//! The tests need a running installation, so they are ignored by default. Inside the
//! qualification environment, set `VEOVEO_ACCEPTANCE_HUB` to the native HTTPS Hub
//! origin and `VEOVEO_ACCEPTANCE_{MEMBER,ADMIN}_BEARER_ENV` to the names of
//! environment variables holding the prepared bearer/opaque references, then
//! run `cargo nextest run --test acceptance --run-ignored only
//! -E 'not (test(restart_persistence_) or test(a_running_schedule_task_recovers_after_restart))'`. The two restart persistence phases run
//! separately around trusted operator restarts; see the README.
//!
//! A subscription first delivers what it watches as it stands when the subscription
//! opens: a Task's current status, or an update naming each resource. A test that
//! claims a pushed change therefore reads nothing between opening the subscription and
//! the change it waits for, and requires the change to follow that opening delivery.
use anyhow::{Context, Result, bail, ensure};
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, CancelTaskParams, ClientCapabilities,
        ClientConfig, DetailedTask, GetTaskParams, Implementation, ProtocolVersion,
        ReadResourceRequestParams, ResourceContents, ServerNotification, SubscriptionFilter,
        TaskPayload, TaskStatus,
    },
    service::{RunningService, ServiceError, Subscription},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::time::Duration;
use veoveo_hub_sdk::client::{BearerSource as _, StaticBearer};
use veoveo_mcp_time::{
    ConvertTimeOutput, Event, EventState, ExpandScheduleOutput, ScheduleOccurrence, TimeInstant,
    TimeResource, TimeScale, TimeVersion, TimeWindow,
};

#[derive(Clone, Copy)]
enum CallerRole {
    Member,
    Admin,
}

struct AcceptanceBearers {
    member: StaticBearer,
    admin: Option<StaticBearer>,
}

impl AcceptanceBearers {
    /// Injectable lookup keeps input regression tests independent of process state.
    fn load(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        ensure!(
            lookup("VEOVEO_ACCEPTANCE_KEY_DIRECTORY").is_none(),
            "VEOVEO_ACCEPTANCE_KEY_DIRECTORY is retired; supply prepared bearer inputs"
        );
        let member_env = lookup("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV")
            .context("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV must name the member bearer variable")?;
        let admin_env = lookup("VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV");
        ensure!(
            admin_env.as_ref() != Some(&member_env),
            "member and admin bearer selectors must name distinct variables"
        );
        let member = Self::read(&lookup, &member_env, "member")?;
        let admin = admin_env
            .as_deref()
            .map(|name| Self::read(&lookup, name, "admin"))
            .transpose()?;
        ensure!(
            admin.as_ref() != Some(&member),
            "member and admin bearer inputs must be distinct"
        );
        Ok(Self {
            member: StaticBearer::new(member),
            admin: admin.map(StaticBearer::new),
        })
    }

    fn read(lookup: &impl Fn(&str) -> Option<String>, name: &str, role: &str) -> Result<String> {
        ensure!(
            !name.is_empty()
                && name.bytes().enumerate().all(|(index, byte)| {
                    byte == b'_'
                        || byte.is_ascii_alphabetic()
                        || (index > 0 && byte.is_ascii_digit())
                }),
            "{role} bearer selector must be an environment variable name"
        );
        let value = lookup(name).with_context(|| format!("{role} bearer variable is missing"))?;
        ensure!(
            !value.is_empty() && !value.chars().any(char::is_whitespace),
            "{role} bearer variable must contain a nonempty bearer without whitespace"
        );
        Ok(value)
    }

    fn source(&self, role: CallerRole) -> Result<&StaticBearer> {
        match role {
            CallerRole::Member => Ok(&self.member),
            CallerRole::Admin => self
                .admin
                .as_ref()
                .context("VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV is required for admin qualification"),
        }
    }
}

struct Caller;

impl ClientHandler for Caller {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::builder().enable_tasks().build(),
            Implementation::new("veoveo-mcp-time-acceptance", env!("CARGO_PKG_VERSION")),
        )
    }
}

type Client = RunningService<RoleClient, Caller>;

struct Installation {
    hub: url::Url,
    bearers: AcceptanceBearers,
    http: reqwest::Client,
}

impl Installation {
    fn from_env() -> Result<Self> {
        // reqwest is built without a TLS provider; Time uses rustls with ring.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let hub = std::env::var("VEOVEO_ACCEPTANCE_HUB")
            .context("VEOVEO_ACCEPTANCE_HUB names the Hub origin")?;
        let bearers = AcceptanceBearers::load(|name| std::env::var(name).ok())?;
        // Keep platform certificate verification, including provisioned Linux roots.
        let http = reqwest::Client::builder().build()?;
        Ok(Self {
            hub: url::Url::parse(&hub)?,
            bearers,
            http,
        })
    }

    fn mcp(&self) -> Result<url::Url> {
        Ok(self.hub.join("/mcp/local")?)
    }

    async fn connect(&self, role: CallerRole) -> Result<Client> {
        let bearer = self.bearers.source(role)?.caller_bearer().await?;
        let transport = StreamableHttpClientTransport::with_client(
            self.http.clone(),
            StreamableHttpClientTransportConfig::with_uri(self.mcp()?.to_string())
                .auth_header(bearer.expose().to_owned()),
        );
        Ok(Caller
            .serve_with_lifecycle(
                transport,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await?)
    }
}

fn tool(name: &str, arguments: Value) -> Result<CallToolRequestParams> {
    let arguments = arguments
        .as_object()
        .cloned()
        .context("tool arguments are an object")?;
    Ok(CallToolRequestParams::new(format!("time__{name}")).with_arguments(arguments))
}

async fn call(client: &Client, name: &str, arguments: Value) -> Result<Value> {
    let result = client.call_tool(tool(name, arguments)?).await?;
    ensure!(
        !result.is_error.unwrap_or(false),
        "{name} returned an error: {:?}",
        result.content
    );
    result.structured_content.context("structured result")
}

async fn read(client: &Client, uri: &str) -> Result<Value> {
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let text = result
        .contents
        .iter()
        .find_map(|content| match content {
            ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .context("text resource")?;
    Ok(serde_json::from_str(text)?)
}

/// The current instant as Time resolves it, carrying the tenant's authority binding.
async fn now(client: &Client) -> Result<Value> {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let resolved = call(
        client,
        "resolve_time",
        json!({"expression": {"format": "rfc3339", "value": now}}),
    )
    .await?;
    Ok(resolved["instant"].clone())
}

fn later(instant: &Value, seconds: i64) -> Result<Value> {
    let mut later = instant.clone();
    let tai = later["taiSecondsSince1970"]
        .as_i64()
        .context("TAI seconds")?;
    later["taiSecondsSince1970"] = json!(tai + seconds);
    Ok(later)
}

async fn create_event(client: &Client, name: &str, due: Value) -> Result<Event> {
    let created = call(
        client,
        "create_event",
        json!({"name": name, "due": due, "idempotencyKey": uuid::Uuid::now_v7().to_string()}),
    )
    .await?;
    let product: veoveo_hub_sdk::definition::Product<Event> = serde_json::from_value(created)?;
    Ok(product.metadata().clone())
}

/// The address `create_event` returned as the event's `resultUri`.
fn event_uri(event: &Event) -> String {
    veoveo_mcp_time::TimeResource::Event(event.event_id.clone()).to_string()
}

async fn event(client: &Client, event: &Event) -> Result<Event> {
    Ok(serde_json::from_value(
        read(client, &event_uri(event)).await?,
    )?)
}

/// Subscribe to one event's own resource; the Hub must accept it unchanged.
async fn watch_event(client: &Client, event: &Event) -> Result<Subscription> {
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([event_uri(event)])
        .build();
    let listener = client.listen(filter.clone()).await?;
    ensure!(
        listener.acknowledged() == &filter,
        "the Hub did not acknowledge the event subscription"
    );
    Ok(listener)
}

/// The next update naming the watched event; it carries no state, so the caller reads it.
async fn next_event_update(listener: &mut Subscription, event: &Event) -> Result<()> {
    let uri = event_uri(event);
    loop {
        let notification = listener
            .next()
            .await?
            .context("the event subscription ended")?;
        if let ServerNotification::ResourceUpdatedNotification(update) = notification {
            ensure!(
                update.params.uri == uri,
                "a notification names {}",
                update.params.uri
            );
            return Ok(());
        }
    }
}

/// The Hub refuses a tool call with a protocol error or a tool error result. A
/// transport failure proves no refusal.
async fn refused(client: &Client, name: &str) -> Result<()> {
    match client.call_tool(tool(name, json!({}))?).await {
        Err(ServiceError::McpError(_)) => Ok(()),
        Ok(result) if result.is_error.unwrap_or(false) => Ok(()),
        Ok(_) => bail!("time__{name} was served"),
        Err(error) => Err(error).context(format!("time__{name} failed without a refusal")),
    }
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn member_discovers_and_calls_time_through_the_hub() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(CallerRole::Member).await?;
    let tools = member.list_all_tools().await?;
    for name in [
        "resolve_time",
        "convert_time",
        "expand_schedule",
        "create_event",
        "cancel_event",
    ] {
        ensure!(
            tools
                .iter()
                .any(|tool| tool.name == format!("time__{name}")),
            "the Hub does not list time__{name}"
        );
    }
    let instant = now(&member).await?;
    let converted: ConvertTimeOutput = serde_json::from_value(
        call(
            &member,
            "convert_time",
            json!({"instant": instant, "scales": ["utc", "tai"]}),
        )
        .await?,
    )
    .context("convert_time returned no Time conversion")?;
    let scales: Vec<TimeScale> = converted
        .scales
        .iter()
        .map(|representation| representation.scale)
        .collect();
    ensure!(
        scales == [TimeScale::Utc, TimeScale::Tai],
        "convert_time projected {scales:?}"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn the_hub_refuses_unauthenticated_and_undeclared_requests() -> Result<()> {
    let installation = Installation::from_env()?;
    let response = installation
        .http
        .post(installation.mcp()?)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header("MCP-Protocol-Version", "2026-07-28")
        .header("Mcp-Method", "tools/list")
        // A valid modern envelope must pass native MCP inspection before the Hub
        // can refuse its missing application bearer. A proxy 403 is not a pass.
        .body(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/list",
                "params": {"_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {}
                }}
            })
            .to_string(),
        )
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    ensure!(
        response.status() == reqwest::StatusCode::UNAUTHORIZED,
        "an unauthenticated request was answered {}",
        response.status()
    );
    ensure!(
        response
            .headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .is_some_and(|value| value.to_str().is_ok_and(|v| v.starts_with("Bearer"))),
        "a 401 carries no Bearer challenge"
    );
    let member = installation.connect(CallerRole::Member).await?;
    refused(&member, "undeclared_tool").await
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn a_schedule_task_reports_its_status_and_completes_through_the_hub() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(CallerRole::Member).await?;
    let start = now(&member).await?;
    // A daily window first recurring a thousand years ago keeps the Task working
    // while Time scans every day up to the horizon.
    let request = json!({
        "calendar": {
            "calendarId": "calendar-acceptance",
            "version": 1,
            "name": "Acceptance",
            "zoneId": "UTC",
            "windows": [{
                "startLocal": "1026-01-01T09:00:00",
                "endLocal": "1026-01-01T10:00:00",
                "recurrence": {"frequency": "daily"}
            }]
        },
        "horizon": {"start": start, "end": later(&start, 3 * 86_400)?},
        "maximumOccurrences": 10
    });
    let created = match member
        .call_tool_once(tool("expand_schedule", request)?)
        .await?
    {
        CallToolResponse::Task(created) => created.task,
        other => bail!("expand_schedule did not create a Task: {other:?}"),
    };
    let task_id = created.task_id.clone();
    let filter = SubscriptionFilter::builder().task_id(&task_id).build();
    let mut listener = member.listen(filter.clone()).await?;
    ensure!(
        listener.acknowledged() == &filter,
        "the Hub did not acknowledge the Task subscription"
    );
    // Nothing polls the Task until its completion arrives on the subscription. The
    // subscription's first status is the one it opened with. That status must still be
    // Working, so Completed can only reach the caller as a pushed change.
    let completed = tokio::time::timeout(Duration::from_secs(120), async {
        let mut opened_working = false;
        loop {
            let notification = listener
                .next()
                .await?
                .context("the Task subscription ended before the Task completed")?;
            let ServerNotification::TaskStatusNotification(notification) = notification else {
                continue;
            };
            let task: DetailedTask = notification.params.task;
            ensure!(
                task.task.task_id == task_id,
                "a status notification names another Task"
            );
            match task.status() {
                TaskStatus::Working => opened_working = true,
                TaskStatus::Completed => {
                    ensure!(
                        opened_working,
                        "the subscription opened on a Completed Task and pushed no status \
                         change: the schedule finished before its subscription opened"
                    );
                    return Ok::<_, anyhow::Error>(task);
                }
                status => bail!("the schedule Task reported {status:?}"),
            }
        }
    })
    .await
    .context("the Hub pushed no Completed status for the schedule Task within 120 seconds")??;
    let TaskPayload::Completed { result } = &completed.payload else {
        bail!("the pushed Completed status carries no result")
    };
    let output: ExpandScheduleOutput = serde_json::from_value(
        result
            .get("structuredContent")
            .cloned()
            .context("the pushed result has no structured content")?,
    )
    .context("the pushed result is not an expanded schedule")?;
    ensure!(
        !output.occurrences.is_empty(),
        "the schedule has no occurrences"
    );
    // The pushed result is the one the Task retains.
    let retained = member.get_task(GetTaskParams::new(&task_id)).await?.task;
    ensure!(
        retained.payload == completed.payload,
        "tasks/get returns a different outcome than the pushed completion"
    );
    eprintln!(
        "Task {task_id}: pushed Completed with {} occurrences",
        output.occurrences.len()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn a_schedule_task_cancels_through_the_hub() -> Result<()> {
    let installation = Installation::from_env()?;
    cancel_schedule_task(&installation).await?;
    Ok(())
}

async fn cancel_schedule_task(installation: &Installation) -> Result<String> {
    let member = installation.connect(CallerRole::Member).await?;
    let start = now(&member).await?;
    // The engine scans from each window's first recurrence with cooperative stop
    // checks. A finite set of historical windows leaves time to observe Working
    // and cancel through another connection, without a production stall hook.
    let window = json!({
        "startLocal": "1026-01-01T09:00:00",
        "endLocal": "1026-01-01T10:00:00",
        "recurrence": {"frequency": "daily"}
    });
    let request = json!({
        "calendar": {
            "calendarId": "calendar-cancellation-acceptance", "version": 1,
            "name": "Cancellation acceptance", "zoneId": "UTC",
            "windows": vec![window; 64]
        },
        "horizon": {"start": start, "end": later(&start, 3 * 86_400)?},
        "maximumOccurrences": 1000
    });
    let created = match member
        .call_tool_once(tool("expand_schedule", request)?)
        .await?
    {
        CallToolResponse::Task(created) => created.task,
        other => bail!("expand_schedule did not create a cancellable Task: {other:?}"),
    };
    let task_id = created.task_id.clone();
    let outcome = tokio::time::timeout(Duration::from_secs(120), async {
        let filter = SubscriptionFilter::builder().task_id(&task_id).build();
        let mut listener = member.listen(filter.clone()).await?;
        ensure!(
            listener.acknowledged() == &filter,
            "the Hub did not acknowledge the cancellation Task subscription"
        );
        let opened = next_task_status(&mut listener, &task_id).await?;
        ensure!(
            opened.status() == TaskStatus::Working,
            "cancellation needs a Working Task; the subscription opened on {:?}",
            opened.status()
        );

        let canceller = installation.connect(CallerRole::Member).await?;
        // Acknowledgement is only cooperative cancellation intent. The worker
        // must settle Cancelled and push that transition before this test passes.
        canceller
            .cancel_task(CancelTaskParams::new(&task_id))
            .await?;
        let cancelled = loop {
            let update = next_task_status(&mut listener, &task_id).await?;
            match update.status() {
                TaskStatus::Working => {}
                TaskStatus::Cancelled => break update,
                status => bail!("the Task reported {status:?} after cancellation, not Cancelled"),
            }
        };
        ensure!(
            matches!(cancelled.payload, TaskPayload::Cancelled),
            "the cancelled Task carries an unexpected payload"
        );
        let reconnected = installation.connect(CallerRole::Member).await?;
        let retained = reconnected
            .get_task(GetTaskParams::new(&task_id))
            .await?
            .task;
        ensure!(
            retained.task.task_id == task_id && retained.payload == cancelled.payload,
            "a fresh connection did not read the retained Cancelled Task"
        );
        eprintln!("Task {task_id}: pushed Cancelled and retained after reconnect");
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("the schedule Task did not settle Cancelled within 120 seconds")
    .and_then(|result| result);
    // Leave no intentionally running calculation when a connection/assertion fails.
    // The request is finite even if the cleanup RPC cannot reach the installation.
    if outcome.is_err() {
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            member.cancel_task(CancelTaskParams::new(&task_id)),
        )
        .await;
    }
    outcome.map(|()| task_id)
}

async fn next_task_status(listener: &mut Subscription, task_id: &str) -> Result<DetailedTask> {
    loop {
        let notification = listener
            .next()
            .await?
            .with_context(|| format!("Task {task_id} subscription ended: {:?}", listener.end()))?;
        if let ServerNotification::TaskStatusNotification(notification) = notification {
            let task = notification.params.task;
            ensure!(
                task.task.task_id == task_id,
                "a status notification names another Task"
            );
            return Ok(task);
        }
    }
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn administration_is_admitted_for_admins_and_refused_to_members() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(CallerRole::Member).await?;
    let admin = installation.connect(CallerRole::Admin).await?;
    now(&admin).await?;
    for name in ["list_sources", "list_active_authorities"] {
        refused(&member, name)
            .await
            .with_context(|| format!("a member called {name}"))?;
        let page = call(&admin, name, json!({"filters":{}})).await?;
        ensure!(page["items"].is_array(), "{name} returned no page");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn an_event_reaches_due_with_a_notification() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(CallerRole::Member).await?;
    // Due can be pushed only to a subscription that opened while the event was still
    // Scheduled. When the subscription opens on a Due event, the host took longer than
    // the lead to open it, which proves nothing either way, so the test tries again
    // with three times the lead.
    const LEADS: [i64; 3] = [10, 30, 90];
    for lead in LEADS {
        let due = later(&now(&member).await?, lead)?;
        let created = create_event(&member, "Acceptance event", due).await?;
        ensure!(
            created.state == EventState::Scheduled,
            "a new event is not Scheduled"
        );
        // The event's own resource is read only after an update names it, and only that
        // event changes it.
        let mut listener = watch_event(&member, &created).await?;
        let opened = tokio::time::timeout(Duration::from_secs(30), async {
            next_event_update(&mut listener, &created).await?;
            event(&member, &created).await
        })
        .await
        .context("the event subscription did not open within 30 seconds")??;
        match opened.state {
            EventState::Scheduled => {}
            EventState::Due => {
                eprintln!("the subscription opened after a {lead} s lead; trying a longer one");
                continue;
            }
            EventState::Cancelled => bail!("a new event read Cancelled"),
        }
        let wait = Duration::from_secs(u64::try_from(lead)? + 60);
        tokio::time::timeout(wait, async {
            loop {
                next_event_update(&mut listener, &created).await?;
                let current = event(&member, &created).await?;
                match current.state {
                    EventState::Scheduled => {}
                    EventState::Due => {
                        ensure!(
                            current.record_version > opened.record_version,
                            "the Due event kept version {:?}",
                            current.record_version
                        );
                        return Ok::<_, anyhow::Error>(());
                    }
                    EventState::Cancelled => bail!("the event was cancelled"),
                }
            }
        })
        .await
        .with_context(|| {
            format!("the Hub pushed no update of the event's Due state within {wait:?}")
        })??;
        return Ok(());
    }
    bail!("every event was Due before its subscription opened, with leads of {LEADS:?} seconds")
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn a_reconnected_caller_reconciles_current_state() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(CallerRole::Member).await?;
    // The event is due a day ahead, so only the cancellation below changes it.
    let due = later(&now(&member).await?, 86_400)?;
    let created = create_event(&member, "Acceptance reconnect", due).await?;
    // A subscriber sees the event Scheduled, then disconnects. The event then changes
    // while no session watches it.
    let mut listener = watch_event(&member, &created).await?;
    let scheduled = tokio::time::timeout(Duration::from_secs(30), async {
        next_event_update(&mut listener, &created).await?;
        event(&member, &created).await
    })
    .await
    .context("the event subscription did not open within 30 seconds")??;
    ensure!(
        scheduled.state == EventState::Scheduled,
        "the event was {:?} before the caller disconnected",
        scheduled.state
    );
    drop(listener);
    member.cancel().await?;
    // Another session without a subscription cancels the event. Its result is the
    // changed event, so the change is complete before the caller reconnects.
    let other = installation.connect(CallerRole::Member).await?;
    let cancelled: Event = serde_json::from_value(
        call(
            &other,
            "cancel_event",
            json!({
                "eventId": scheduled.event_id,
                "expectedRecordVersion": scheduled.record_version,
            }),
        )
        .await?,
    )
    .context("cancel_event returned no event")?;
    ensure!(
        cancelled.state == EventState::Cancelled
            && cancelled.record_version > scheduled.record_version,
        "cancel_event returned a {:?} event at version {:?}",
        cancelled.state,
        cancelled.record_version
    );
    other.cancel().await?;
    // The new session's subscription opens with an update naming the event. Reading
    // after it returns the current state, which no update delivered while disconnected.
    let member = installation.connect(CallerRole::Member).await?;
    let mut listener = watch_event(&member, &created).await?;
    let current = tokio::time::timeout(Duration::from_secs(30), async {
        next_event_update(&mut listener, &created).await?;
        event(&member, &created).await
    })
    .await
    .context("the reconnected subscription did not open within 30 seconds")??;
    ensure!(
        current == cancelled,
        "the event read after reconnecting is {:?} at version {:?}, not the cancelled event",
        current.state,
        current.record_version
    );
    Ok(())
}

/// Run alone with streamed native Exec stdin; the operator owns lifecycle control.
#[tokio::test]
#[ignore = "requires a coordinated external Time restart and an exact continue line"]
async fn a_running_schedule_task_recovers_after_restart() -> Result<()> {
    let installation = Installation::from_env()?;
    let mut cleanup_task = None;
    let outcome = tokio::time::timeout(Duration::from_secs(240), async {
        let member = installation.connect(CallerRole::Member).await?;
        // One complete future UTC day makes the expected result independent of
        // the wall-clock time consumed by the restart. Historical recurrences
        // provide finite work with ordinary cooperative checkpoints.
        let day = chrono::Utc::now().date_naive().checked_add_days(chrono::Days::new(2))
            .context("the restart schedule day overflowed")?;
        let next_day = day.succ_opt().context("the restart horizon day overflowed")?;
        let horizon_start = resolve_instant(&member, format!("{day}T00:00:00Z")).await?;
        let horizon_end = resolve_instant(&member, format!("{next_day}T00:00:00Z")).await?;
        let expected_start = resolve_instant(&member, format!("{day}T09:00:00Z")).await?;
        let expected_end = resolve_instant(&member, format!("{day}T10:00:00Z")).await?;
        let expected_window = TimeWindow::new(expected_start, expected_end)?;
        const WINDOWS: u32 = 64;
        let expected = ExpandScheduleOutput {
            occurrences: (0..WINDOWS).map(|sequence| ScheduleOccurrence {
                sequence, window: expected_window.clone(), labels: vec!["restart-acceptance".into()]
            }).collect(),
            truncated: false,
        };
        let window = json!({
            "startLocal": "1026-01-01T09:00:00", "endLocal": "1026-01-01T10:00:00",
            "recurrence": {"frequency": "daily"}, "labels": ["restart-acceptance"]
        });
        let request = json!({
            "calendar": {"calendarId": "calendar-restart-acceptance", "version": 1,
                "name": "Restart acceptance", "zoneId": "UTC", "windows": vec![window; usize::try_from(WINDOWS)?]},
            "horizon": TimeWindow::new(horizon_start, horizon_end)?, "maximumOccurrences": 1000
        });
        let created = match member.call_tool_once(tool("expand_schedule", request)?).await? {
            CallToolResponse::Task(created) => created.task,
            other => bail!("expand_schedule did not create a restart Task: {other:?}"),
        };
        let task_id = created.task_id;
        cleanup_task = Some(task_id.clone());
        let filter = SubscriptionFilter::builder().task_id(&task_id).build();
        let mut initial = member.listen(filter.clone()).await?;
        ensure!(initial.acknowledged() == &filter, "the Hub did not acknowledge the restart Task subscription");
        let working = next_task_status(&mut initial, &task_id).await?;
        ensure!(working.status() == TaskStatus::Working,
            "inconclusive restart evidence: the initial Task is {:?}, not Working", working.status());
        drop(initial);
        member.cancel().await?;
        println!("Working Task {task_id}; ready for trusted Time restart; waiting for continue");
        std::io::Write::flush(&mut std::io::stdout()).context("flushing restart readiness")?;
        continue_after_restart().await?;

        // The operator has proved the old runtime stopped and the new one is
        // ready. Requiring nonterminal state now rules out completion before
        // that boundary; lastUpdatedAt is not a completion timestamp.
        let reconnected = installation.connect(CallerRole::Member).await?;
        let current = reconnected.get_task(GetTaskParams::new(&task_id)).await
            .context("reading the same Task after the operator restart boundary")?.task;
        ensure!(current.task.task_id == task_id && current.status() == TaskStatus::Working,
            "inconclusive restart evidence: after the operator boundary the same Task is {:?}, not Working", current.status());
        eprintln!("Task {task_id}: Working after the operator restart boundary");
        let mut resumed = reconnected.listen(filter.clone()).await
            .context("opening the Task subscription after the operator restart boundary")?;
        ensure!(resumed.acknowledged() == &filter, "the Hub did not acknowledge the resumed Task subscription");
        let opened = next_task_status(&mut resumed, &task_id).await
            .context("waiting for the reopened Task subscription's initial Working state")?;
        ensure!(opened.status() == TaskStatus::Working,
            "inconclusive restart evidence: the new subscription opened on {:?}, not Working", opened.status());
        eprintln!("Task {task_id}: reopened subscription delivered Working; waiting for pushed Completed");
        let completed = loop {
            let update = next_task_status(&mut resumed, &task_id).await
                .context("waiting for the resumed Task's pushed Completed state")?;
            match update.status() {
                TaskStatus::Working => {}
                TaskStatus::Completed => break update,
                status => bail!("the restarted schedule Task reported {status:?}"),
            }
        };
        let TaskPayload::Completed { result } = &completed.payload else {
            bail!("the resumed Completed Task carries no result")
        };
        let expanded: ExpandScheduleOutput = serde_json::from_value(result.get("structuredContent")
            .cloned().context("the resumed result has no structured content")?)?;
        ensure!(expanded == expected, "the resumed schedule differs from the expected UTC windows");
        let retained = reconnected.get_task(GetTaskParams::new(&task_id)).await
            .context("reading the retained resumed Task completion")?.task;
        ensure!(retained.task.task_id == task_id && retained.payload == completed.payload,
            "tasks/get did not retain the resumed completion");
        eprintln!("Task {task_id}: Working after confirmed restart, pushed Completed with expected schedule, completion retained");
        Ok::<_, anyhow::Error>(())
    }).await.context("the coordinated restart probe exceeded 240 seconds").and_then(|result| result);
    if outcome.is_err()
        && let Some(task_id) = cleanup_task
    {
        let _ = tokio::time::timeout(Duration::from_secs(10), async {
            let member = installation.connect(CallerRole::Member).await?;
            member.cancel_task(CancelTaskParams::new(task_id)).await?;
            Ok::<_, anyhow::Error>(())
        })
        .await;
    }
    outcome
}

async fn resolve_instant(client: &Client, value: String) -> Result<TimeInstant> {
    let resolved = call(
        client,
        "resolve_time",
        json!({"expression": {"format": "rfc3339", "value": value}}),
    )
    .await?;
    Ok(serde_json::from_value(resolved["instant"].clone())?)
}

async fn continue_after_restart() -> Result<()> {
    let (send, receive) = tokio::sync::oneshot::channel();
    // Tokio stdin's blocking read cannot be cancelled and can delay runtime
    // shutdown. A detached standard thread lets the isolated test process exit
    // on its deadline even if the operator never supplies input.
    std::thread::Builder::new()
        .name("restart-continue".into())
        .spawn(move || {
            let mut input = std::io::Read::take(std::io::stdin().lock(), 64);
            let mut line = Vec::new();
            let result = std::io::BufRead::read_until(&mut input, b'\n', &mut line)
                .context("reading restart continuation")
                .and_then(|_| {
                    ensure!(
                        line == b"continue\n",
                        "restart continuation must be exactly continue followed by LF"
                    );
                    Ok(())
                });
            let _ = send.send(result);
        })
        .context("starting the bounded restart continuation reader")?;
    receive
        .await
        .context("the restart continuation reader ended")?
}

/// Prepare terminal domain state; the trusted operator owns the restart boundary.
#[tokio::test]
#[ignore = "explicit prepare phase before trusted Time and Hub restarts"]
async fn restart_persistence_prepare() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(180), async {
        let installation = Installation::from_env()?;
        let member = installation.connect(CallerRole::Member).await?;
        let created = create_event(
            &member,
            "Acceptance restart persistence",
            later(&now(&member).await?, 86_400)?,
        )
        .await?;
        let cancelled: Event = serde_json::from_value(
            call(
                &member,
                "cancel_event",
                json!({
                    "eventId": created.event_id, "expectedRecordVersion": created.record_version
                }),
            )
            .await?,
        )?;
        ensure!(
            cancelled.state == EventState::Cancelled
                && cancelled.record_version == created.record_version.checked_next()?,
            "restart preparation did not retain the cancelled event version"
        );
        ensure!(
            event(&member, &created).await? == cancelled,
            "restart preparation read a different event after cancellation"
        );
        let task_id = cancel_schedule_task(&installation).await?;
        // Non-secret standard identifiers/state only. No local manifest, operator
        // credentials or restart control enters this process.
        println!(
            "VEOVEO_ACCEPTANCE_RESTART_EVENT_URI={}",
            event_uri(&created)
        );
        println!(
            "VEOVEO_ACCEPTANCE_RESTART_EVENT_VERSION={}",
            cancelled.record_version.get()
        );
        println!("VEOVEO_ACCEPTANCE_RESTART_TASK_ID={task_id}");
        println!("VEOVEO_ACCEPTANCE_RESTART_TASK_STATUS=cancelled");
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("restart preparation exceeded 180 seconds")?
}

/// Verify only after the operator proves actual restarts and readiness separately.
#[tokio::test]
#[ignore = "explicit verify phase after trusted Time and Hub restarts"]
async fn restart_persistence_verify() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let installation = Installation::from_env()?;
        let inputs = RestartInputs::load(|name| std::env::var(name).ok())?;
        let member = installation.connect(CallerRole::Member).await?;
        let current: Event =
            serde_json::from_value(read(&member, &inputs.event_uri.to_string()).await?)?;
        let TimeResource::Event(expected_id) = &inputs.event_uri else {
            unreachable!("validated event URI")
        };
        ensure!(
            current.event_id == *expected_id
                && current.state == EventState::Cancelled
                && current.record_version == inputs.event_version,
            "the prepared event identity/state/version did not survive the restart boundary"
        );
        let retained = member
            .get_task(GetTaskParams::new(&inputs.task_id))
            .await?
            .task;
        ensure!(
            retained.task.task_id == inputs.task_id
                && matches!(retained.payload, TaskPayload::Cancelled),
            "the prepared Task did not retain its Cancelled outcome"
        );

        let scheduled = current.clone();
        let mut events = watch_event(&member, &scheduled).await?;
        next_event_update(&mut events, &scheduled).await?;
        ensure!(
            event(&member, &scheduled).await? == current,
            "a fresh event subscription reconciled a different retained event"
        );
        let filter = SubscriptionFilter::builder()
            .task_id(&inputs.task_id)
            .build();
        let mut tasks = member.listen(filter.clone()).await?;
        ensure!(
            tasks.acknowledged() == &filter,
            "the Hub did not acknowledge the retained Task subscription"
        );
        let opened = next_task_status(&mut tasks, &inputs.task_id).await?;
        ensure!(
            opened.payload == retained.payload,
            "a fresh Task subscription reconciled a different retained outcome"
        );
        eprintln!(
            "prepared event/version and Cancelled Task retained; fresh subscriptions reconciled"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("restart verification exceeded 60 seconds")?
}

struct RestartInputs {
    event_uri: TimeResource,
    event_version: TimeVersion,
    task_id: String,
}

impl RestartInputs {
    fn load(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let required = |name| {
            lookup(name).with_context(|| format!("{name} is required from restart preparation"))
        };
        let event_uri = TimeResource::parse(&required("VEOVEO_ACCEPTANCE_RESTART_EVENT_URI")?)?;
        ensure!(
            matches!(event_uri, TimeResource::Event(_)),
            "restart event URI must name one event"
        );
        let event_version =
            TimeVersion::new(required("VEOVEO_ACCEPTANCE_RESTART_EVENT_VERSION")?.parse()?)?;
        let task_id = required("VEOVEO_ACCEPTANCE_RESTART_TASK_ID")?;
        ensure!(
            !task_id.is_empty() && !task_id.chars().any(char::is_whitespace),
            "restart Task ID must be nonempty without whitespace"
        );
        ensure!(
            required("VEOVEO_ACCEPTANCE_RESTART_TASK_STATUS")? == "cancelled",
            "this persistence case requires the prepared Cancelled terminal Task outcome"
        );
        Ok(Self {
            event_uri,
            event_version,
            task_id,
        })
    }
}

#[cfg(test)]
mod bearer_inputs {
    use super::*;
    use std::collections::BTreeMap;

    fn load(entries: &[(&str, &str)]) -> Result<AcceptanceBearers> {
        let values: BTreeMap<_, _> = entries.iter().copied().collect();
        AcceptanceBearers::load(|name| values.get(name).map(|value| (*value).to_owned()))
    }

    #[tokio::test]
    async fn selects_each_role_without_exposing_bearers() -> Result<()> {
        let bearers = load(&[
            ("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE"),
            ("VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV", "ADMIN_REFERENCE"),
            ("MEMBER_REFERENCE", "opaque-member-reference"),
            ("ADMIN_REFERENCE", "opaque-admin-reference"),
        ])?;
        assert_eq!(
            bearers.source(CallerRole::Member)?.caller_bearer().await?,
            veoveo_hub_sdk::client::Bearer::new("opaque-member-reference")
        );
        assert_eq!(
            bearers.source(CallerRole::Admin)?.caller_bearer().await?,
            veoveo_hub_sdk::client::Bearer::new("opaque-admin-reference")
        );
        Ok(())
    }

    #[test]
    fn member_only_input_does_not_require_admin_custody() -> Result<()> {
        let bearers = load(&[
            ("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE"),
            ("MEMBER_REFERENCE", "opaque-member-reference"),
        ])?;
        assert!(bearers.source(CallerRole::Member).is_ok());
        assert!(
            bearers
                .source(CallerRole::Admin)
                .err()
                .context("admin must be unavailable")?
                .to_string()
                .contains("ADMIN_BEARER_ENV")
        );
        Ok(())
    }

    #[test]
    fn rejects_missing_invalid_ambiguous_and_retired_inputs_without_secret_diagnostics() {
        let cases: &[&[(&str, &str)]] = &[
            &[],
            &[("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE")],
            &[("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "secret-selector!")],
            &[("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "9INVALID")],
            &[
                ("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE"),
                ("MEMBER_REFERENCE", ""),
            ],
            &[
                ("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE"),
                ("MEMBER_REFERENCE", "secret with-whitespace"),
            ],
            &[
                ("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE"),
                ("VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV", "MEMBER_REFERENCE"),
                ("MEMBER_REFERENCE", "secret-duplicate"),
            ],
            &[
                ("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE"),
                ("VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV", "ADMIN_REFERENCE"),
                ("MEMBER_REFERENCE", "secret-duplicate"),
                ("ADMIN_REFERENCE", "secret-duplicate"),
            ],
            &[
                ("VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV", "MEMBER_REFERENCE"),
                ("VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV", "ADMIN_REFERENCE"),
                ("MEMBER_REFERENCE", "opaque-member-reference"),
            ],
            &[("VEOVEO_ACCEPTANCE_KEY_DIRECTORY", "secret-key-path")],
        ];
        let expected = [
            "MEMBER_BEARER_ENV must name",
            "member bearer variable is missing",
            "member bearer selector must be an environment variable name",
            "member bearer selector must be an environment variable name",
            "member bearer variable must contain a nonempty bearer",
            "member bearer variable must contain a nonempty bearer",
            "selectors must name distinct variables",
            "bearer inputs must be distinct",
            "admin bearer variable is missing",
            "KEY_DIRECTORY is retired",
        ];
        assert_eq!(cases.len(), expected.len());
        for (entries, expected) in cases.iter().zip(expected) {
            let error = load(entries)
                .err()
                .expect("invalid input must fail")
                .to_string();
            assert!(
                !error.contains("secret"),
                "credential data reached diagnostics"
            );
            assert!(
                error.contains(expected),
                "input rejected for the wrong reason: {error}"
            );
        }
    }
}

#[cfg(test)]
mod restart_inputs {
    use super::*;
    use std::collections::BTreeMap;

    fn prepared() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([
            (
                "VEOVEO_ACCEPTANCE_RESTART_EVENT_URI",
                "time://events/event-restart-acceptance",
            ),
            ("VEOVEO_ACCEPTANCE_RESTART_EVENT_VERSION", "2"),
            ("VEOVEO_ACCEPTANCE_RESTART_TASK_ID", "prepared-task-id"),
            ("VEOVEO_ACCEPTANCE_RESTART_TASK_STATUS", "cancelled"),
        ])
    }

    fn load(values: &BTreeMap<&str, &str>) -> Result<RestartInputs> {
        RestartInputs::load(|name| values.get(name).map(|value| (*value).to_owned()))
    }

    #[test]
    fn selects_exact_event_version_and_terminal_task_expectation() -> Result<()> {
        let inputs = load(&prepared())?;
        assert_eq!(
            inputs.event_uri.to_string(),
            "time://events/event-restart-acceptance"
        );
        assert_eq!(inputs.event_version, TimeVersion::new(2)?);
        assert_eq!(inputs.task_id, "prepared-task-id");
        Ok(())
    }

    #[test]
    fn rejects_missing_or_ambiguous_restart_expectations() {
        for (name, value) in [
            (
                "VEOVEO_ACCEPTANCE_RESTART_EVENT_URI",
                "time://clock/current",
            ),
            ("VEOVEO_ACCEPTANCE_RESTART_EVENT_VERSION", "0"),
            ("VEOVEO_ACCEPTANCE_RESTART_TASK_ID", ""),
            ("VEOVEO_ACCEPTANCE_RESTART_TASK_ID", "two tasks"),
            ("VEOVEO_ACCEPTANCE_RESTART_TASK_STATUS", "completed"),
        ] {
            let mut values = prepared();
            values.insert(name, value);
            assert!(load(&values).is_err(), "accepted invalid {name}");
        }
        for name in prepared().keys() {
            let mut values = prepared();
            values.remove(name);
            assert!(load(&values).is_err(), "accepted missing {name}");
        }
    }
}
