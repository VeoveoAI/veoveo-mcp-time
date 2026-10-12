//! Acceptance through a running local installation: Time registered with the Gateway and
//! called through the Gateway by the installation's member and admin callers.
//!
//! The tests need a running installation, so they are ignored by default. Inside the
//! Time environment, set `VEOVEO_ACCEPTANCE_GATEWAY` to the Gateway origin and
//! `VEOVEO_ACCEPTANCE_KEY_DIRECTORY` to the directory holding the callers' keys, then
//! run `cargo nextest run --test acceptance --run-ignored only`.
use anyhow::{Context, Result, bail, ensure};
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig, GetTaskParams,
        Implementation, ProtocolVersion, ReadResourceRequestParams, ResourceContents,
        ServerNotification, SubscriptionFilter, TaskPayload, TaskStatus,
    },
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use veoveo_mcp_time::{Event, EventState};
use veoveo_oauth_client::{ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_types::OAuthClientId;

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
    gateway: url::Url,
    keys: PathBuf,
}

impl Installation {
    fn from_env() -> Result<Self> {
        // reqwest is built without a TLS provider; Time uses rustls with ring.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let gateway = std::env::var("VEOVEO_ACCEPTANCE_GATEWAY")
            .context("VEOVEO_ACCEPTANCE_GATEWAY names the Gateway origin")?;
        let keys = std::env::var("VEOVEO_ACCEPTANCE_KEY_DIRECTORY")
            .context("VEOVEO_ACCEPTANCE_KEY_DIRECTORY names the callers' key directory")?;
        Ok(Self {
            gateway: url::Url::parse(&gateway)?,
            keys: PathBuf::from(keys),
        })
    }

    fn mcp(&self) -> Result<url::Url> {
        Ok(self.gateway.join("/mcp/local")?)
    }

    /// An RFC 6749 client-credentials token, authenticated with an RFC 7523 assertion.
    async fn token(&self, caller: &str) -> Result<String> {
        let jwks: Value = serde_json::from_slice(&std::fs::read(
            self.keys.join(format!("{caller}.jwks.json")),
        )?)?;
        let kid = jwks["keys"][0]["kid"]
            .as_str()
            .context("caller key identifier")?;
        let pem = std::fs::read(self.keys.join(format!("{caller}.pem")))?;
        let signer = ClientAssertionSigner::from_rsa_pem(ClientAssertionKeyId::parse(kid)?, &pem)?;
        let endpoint = self.gateway.join("/oauth/token")?;
        let assertion = signer.assertion(&OAuthClientId::parse(caller)?, &endpoint)?;
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "client_credentials")
            .append_pair("client_id", caller)
            .append_pair(
                "client_assertion_type",
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            )
            .append_pair("client_assertion", assertion.expose_secret())
            .append_pair("resource", self.mcp()?.as_str())
            .append_pair("scope", "mcp")
            .finish();
        let response = reqwest::Client::new()
            .post(endpoint)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await?;
        let status = response.status();
        ensure!(status.is_success(), "token endpoint answered {status}");
        let token: Value = response.json().await?;
        Ok(token["access_token"]
            .as_str()
            .context("access_token")?
            .to_owned())
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

async fn create_event(client: &Client, name: &str, due: Value) -> Result<Event> {
    let created = call(
        client,
        "create_event",
        json!({"name": name, "due": due, "idempotencyKey": uuid::Uuid::now_v7().to_string()}),
    )
    .await?;
    Ok(serde_json::from_value(created)?)
}

async fn event(client: &Client, event: &Event) -> Result<Event> {
    Ok(serde_json::from_value(
        read(client, &format!("time://events/{}", event.event_id)).await?,
    )?)
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn member_discovers_and_calls_time_through_the_gateway() -> Result<()> {
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
            "the Gateway does not list time__{name}"
        );
    }
    let instant = now(&member).await?;
    let converted = call(
        &member,
        "convert_time",
        json!({"instant": instant, "scales": ["utc", "tai"]}),
    )
    .await?;
    ensure!(converted.is_object(), "convert_time returned no projection");
    let admin = installation.connect(ADMIN).await?;
    now(&admin).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn the_gateway_refuses_unauthenticated_and_undeclared_requests() -> Result<()> {
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
    let undeclared = member.call_tool(tool("undeclared_tool", json!({}))?).await;
    match undeclared {
        Err(_) => {}
        Ok(result) => ensure!(
            result.is_error.unwrap_or(false),
            "an undeclared tool was served"
        ),
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn a_schedule_task_reports_its_status_and_completes_through_the_gateway() -> Result<()> {
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
        "the Gateway did not acknowledge the Task subscription"
    );
    let current = member.get_task(GetTaskParams::new(&task_id)).await?.task;
    ensure!(
        matches!(current.status(), TaskStatus::Working),
        "the schedule Task finished before its subscription opened: {:?}",
        current.status()
    );
    let notified = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            match listener
                .next()
                .await?
                .context("the Task subscription ended before a status")?
            {
                ServerNotification::TaskStatusNotification(notification) => {
                    ensure!(
                        notification.params.task.task.task_id == task_id,
                        "a status notification names another Task"
                    );
                    return Ok::<_, anyhow::Error>(notification.params.task.status());
                }
                _ => continue,
            }
        }
    })
    .await
    .context("no Task status notification within 120 seconds")??;
    let completed = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let current = member.get_task(GetTaskParams::new(&task_id)).await?.task;
            if !matches!(current.status(), TaskStatus::Working) {
                return Ok::<_, anyhow::Error>(current);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .context("the schedule Task did not finish within 120 seconds")??;
    ensure!(
        matches!(completed.status(), TaskStatus::Completed),
        "the schedule Task ended {:?}",
        completed.status()
    );
    let TaskPayload::Completed { result } = completed.payload else {
        bail!("a completed Task carries no result")
    };
    let occurrences = result
        .get("structuredContent")
        .and_then(|content| content.get("occurrences"))
        .and_then(Value::as_array)
        .context("schedule occurrences")?;
    ensure!(!occurrences.is_empty(), "the schedule has no occurrences");
    eprintln!(
        "Task {task_id}: notified {notified:?}, {} occurrences",
        occurrences.len()
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
        match member.call_tool(tool(name, json!({}))?).await {
            Err(_) => {}
            Ok(result) => ensure!(
                result.is_error.unwrap_or(false),
                "a member was admitted to {name}"
            ),
        }
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
    let mut listener = member
        .listen(
            SubscriptionFilter::builder()
                .resource_subscription("time://events")
                .build(),
        )
        .await?;
    let due = later(&now(&member).await?, 5)?;
    let created = create_event(&member, "Acceptance event", due).await?;
    ensure!(
        created.state == EventState::Scheduled,
        "a new event is not Scheduled"
    );
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let notification = listener
                .next()
                .await?
                .context("the event subscription ended before Due")?;
            if let ServerNotification::ResourceUpdatedNotification(update) = notification {
                ensure!(
                    update.params.uri.starts_with("time://events"),
                    "a notification names {}",
                    update.params.uri
                );
                if event(&member, &created).await?.state == EventState::Due {
                    return Ok::<_, anyhow::Error>(());
                }
            }
        }
    })
    .await
    .context("the event was not delivered as Due within 60 seconds")??;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a running local installation"]
async fn a_reconnected_caller_reconciles_current_state() -> Result<()> {
    let installation = Installation::from_env()?;
    let member = installation.connect(MEMBER).await?;
    let due = later(&now(&member).await?, 3)?;
    let created = create_event(&member, "Acceptance reconnect", due).await?;
    member.cancel().await?;
    tokio::time::sleep(Duration::from_secs(8)).await;
    let member = installation.connect(MEMBER).await?;
    let current = event(&member, &created).await?;
    ensure!(
        current.state == EventState::Due,
        "the event read after reconnecting is {:?}",
        current.state
    );
    Ok(())
}
