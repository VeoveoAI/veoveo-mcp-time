//! Acceptance through a running local installation: Time registered with the Hub and
//! called through the Hub by the installation's member and admin callers.
//!
//! The tests need a running installation, so they are ignored by default. Inside the
//! Time environment, set `VEOVEO_ACCEPTANCE_HUB` to the Hub origin and
//! `VEOVEO_ACCEPTANCE_KEY_DIRECTORY` to the directory holding the callers' keys, then
//! run `cargo nextest run --test acceptance --run-ignored only`.
//!
//! A subscription first delivers what it watches as it stands when the subscription
//! opens: a Task's current status, or an update naming each resource. A test that
//! claims a pushed change therefore reads nothing between opening the subscription and
//! the change it waits for, and requires the change to follow that opening delivery.
use anyhow::{Context, Result, bail, ensure};
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig, DetailedTask,
        GetTaskParams, Implementation, ProtocolVersion, ReadResourceRequestParams,
        ResourceContents, ServerNotification, SubscriptionFilter, TaskPayload, TaskStatus,
    },
    service::{RunningService, ServiceError, Subscription},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use veoveo_hub_sdk::client::{BearerSource as _, ClientCredentialsBearer, caller_signer};
use veoveo_mcp_time::{
    ConvertTimeOutput, Event, EventState, ExpandScheduleOutput, ScheduledEvent, TimeScale,
};
use veoveo_types::{OAuthClientId, ScopeName};

const MEMBER: &str = "acceptance-member";
const ADMIN: &str = "acceptance-admin";

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
    keys: PathBuf,
}

impl Installation {
    fn from_env() -> Result<Self> {
        // reqwest is built without a TLS provider; Time uses rustls with ring.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let hub = std::env::var("VEOVEO_ACCEPTANCE_HUB")
            .context("VEOVEO_ACCEPTANCE_HUB names the Hub origin")?;
        let keys = std::env::var("VEOVEO_ACCEPTANCE_KEY_DIRECTORY")
            .context("VEOVEO_ACCEPTANCE_KEY_DIRECTORY names the callers' key directory")?;
        Ok(Self {
            hub: url::Url::parse(&hub)?,
            keys: PathBuf::from(keys),
        })
    }

    fn mcp(&self) -> Result<url::Url> {
        Ok(self.hub.join("/mcp/local")?)
    }

    /// The caller's client-credentials token for the profile, through the SDK's bearer,
    /// which discovers the token endpoint from the profile's metadata.
    async fn token(&self, caller: &str) -> Result<String> {
        let caller = OAuthClientId::parse(caller)?;
        let signer = caller_signer(&self.keys, &caller)?;
        let bearer = ClientCredentialsBearer::discover(
            &self.mcp()?,
            caller,
            signer,
            ScopeName::parse("mcp")?,
            reqwest::Client::new(),
        )
        .await?;
        Ok(bearer.caller_bearer().await?.expose().to_owned())
    }

    async fn connect(&self, caller: &str) -> Result<Client> {
        let token = self.token(caller).await?;
        let transport = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(self.mcp()?.to_string())
                .auth_header(token),
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

async fn create_event(client: &Client, name: &str, due: Value) -> Result<ScheduledEvent> {
    let created = call(
        client,
        "create_event",
        json!({"name": name, "due": due, "idempotencyKey": uuid::Uuid::now_v7().to_string()}),
    )
    .await?;
    Ok(serde_json::from_value(created)?)
}

/// The address `create_event` returned as the event's `resultUri`.
fn event_uri(event: &ScheduledEvent) -> String {
    event.result_uri.to_string()
}

async fn event(client: &Client, event: &ScheduledEvent) -> Result<Event> {
    Ok(serde_json::from_value(
        read(client, &event_uri(event)).await?,
    )?)
}

/// Subscribe to one event's own resource; the Hub must accept it unchanged.
async fn watch_event(client: &Client, event: &ScheduledEvent) -> Result<Subscription> {
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
async fn next_event_update(listener: &mut Subscription, event: &ScheduledEvent) -> Result<()> {
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
    let member = installation.connect(MEMBER).await?;
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
    let admin = installation.connect(ADMIN).await?;
    now(&admin).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn the_hub_refuses_unauthenticated_and_undeclared_requests() -> Result<()> {
    let installation = Installation::from_env()?;
    let response = reqwest::Client::new()
        .post(installation.mcp()?)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#)
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
    let member = installation.connect(MEMBER).await?;
    refused(&member, "undeclared_tool").await
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn a_schedule_task_reports_its_status_and_completes_through_the_hub() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(MEMBER).await?;
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
async fn administration_is_admitted_for_admins_and_refused_to_members() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(MEMBER).await?;
    let admin = installation.connect(ADMIN).await?;
    for name in ["list_sources", "list_active_authorities"] {
        refused(&member, name)
            .await
            .with_context(|| format!("a member called {name}"))?;
        let page = call(&admin, name, json!({})).await?;
        ensure!(page["items"].is_array(), "{name} returned no page");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn an_event_reaches_due_with_a_notification() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(MEMBER).await?;
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
    let member = installation.connect(MEMBER).await?;
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
    let other = installation.connect(MEMBER).await?;
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
    let member = installation.connect(MEMBER).await?;
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
