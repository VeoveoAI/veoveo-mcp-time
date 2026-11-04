//! Time's knowledge search through the SDK's in-process mesh, over a fresh database with
//! pgvector. Embedding is a synthetic stand-in on the mesh that serves Embedding's own
//! `embed` contract, so Time's `EmbeddingClient` calls it as it calls Embedding: it
//! answers a vector hashed from an item's words, and from a media part's media type,
//! never a model's. These tests prove the indexing path, the ranking and the read rule,
//! not retrieval quality.
use super::{hosted::app_html, *};
use std::sync::{LazyLock, Mutex};

use rmcp::{
    RoleServer,
    model::{Implementation, ServerConfig},
    service::RequestContext,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use veoveo_hub_sdk::{
    docs::ServerDocs,
    durable_tasks::DurableTasks,
    hosting::{
        DomainServer, Hosted, RoutePolicy, Routes, ToolError, hub_identity,
        testing::{self, RunningMesh, TestMesh},
    },
    knowledge::ContentPart,
    server_contract::{McpServerContract, McpServerSetup},
};
use veoveo_mcp_embedding::contract::{
    Device, EffectiveProcessing, EmbedRequest, Embedding, EmbeddingPermission, EmbeddingProvider,
    EmbeddingResult, Execution, FfmpegIdentity, ModelIdentity, RuntimeIdentity, TokenUsage,
};
use veoveo_types::{PermissionName, ResourceScheme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum NoScope {}

/// The stand-in for Embedding, and the text of each item it embedded.
#[derive(Clone, Default)]
struct Synthetic {
    texts: Arc<Mutex<Vec<String>>>,
}

struct SyntheticContract;
impl McpServerContract for SyntheticContract {
    type Server = Synthetic;
    type Provider = EmbeddingProvider;
    type Scope = NoScope;
    fn scheme() -> ResourceScheme {
        ResourceScheme::parse("embedding").unwrap()
    }
    fn scopes() -> &'static [NoScope] {
        &[]
    }
    fn documents() -> &'static ServerDocs {
        static DOCS: LazyLock<ServerDocs> = LazyLock::new(|| {
            ServerDocs::new(veoveo_types::ServerSlug::parse("embedding").unwrap())
                .with_doc("agents", "Agents", "# Synthetic Embedding")
                .with_doc("design", "Design", "# Synthetic tests only")
        });
        &DOCS
    }
    fn server_config() -> ServerConfig {
        let mut config = ServerConfig::default();
        config.server_info = Implementation::new("embedding", "1");
        config
    }
}

static SYNTHETIC_ROUTES: LazyLock<Routes<Synthetic>> = LazyLock::new(|| {
    Routes::builder(RoutePolicy::Authenticated)
        .tool::<EmbedRequest, _, _>(
            "Synthetic embeddings",
            RoutePolicy::permission(EmbeddingPermission::EmbeddingsCreate).unwrap(),
            |domain: Synthetic, request, context| async move { domain.embed(request, context).await },
        )
        .build()
});

impl DomainServer for Synthetic {
    type Contract = SyntheticContract;
    fn setup() -> &'static McpServerSetup<SyntheticContract> {
        static SETUP: LazyLock<McpServerSetup<SyntheticContract>> =
            LazyLock::new(|| McpServerSetup::new().unwrap());
        &SETUP
    }
    fn routes() -> &'static Routes<Self> {
        &SYNTHETIC_ROUTES
    }
}

fn slot(bytes: &[u8], modulus: usize) -> usize {
    bytes.iter().fold(7_usize, |hash, byte| {
        hash.wrapping_mul(31) + usize::from(*byte)
    }) % modulus
}

impl Synthetic {
    async fn embed(
        self,
        request: EmbedRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<EmbeddingResult, ToolError> {
        hub_identity(&context)?;
        let mut items = Vec::with_capacity(request.items.len());
        for (index, item) in request.items.iter().enumerate() {
            let mut vector = vec![0.0_f32; 768];
            for part in &item.content {
                match part {
                    ContentPart::Text { text } => {
                        self.texts.lock().unwrap().push(text.clone());
                        for word in text.split_whitespace() {
                            vector[slot(word.to_lowercase().as_bytes(), 700)] += 1.0;
                        }
                    }
                    media => {
                        let media = media.signed_media().expect("a media part");
                        vector[700 + slot(media.media_type.as_str().as_bytes(), 60)] += 1.0;
                    }
                }
            }
            if vector.iter().all(|value| *value == 0.0) {
                vector[767] = 1.0;
            }
            items.push(Embedding {
                index,
                vector,
                token_count: 1,
                media: Vec::new(),
            });
        }
        let tokens = items.len() as u64;
        Ok(EmbeddingResult {
            request_id: Uuid::now_v7(),
            model: ModelIdentity {
                id: "synthetic/word-hash".into(),
                revision: "1".into(),
            },
            processing_id: Sha256Digest::from_bytes([0; 32]),
            dimensions: request.dimensions,
            normalized: false,
            execution: Execution {
                device: Device::Mps,
                dtype: "float32".into(),
            },
            runtime: RuntimeIdentity {
                package: "synthetic".into(),
                version: "0".into(),
                source_revision: "synthetic".into(),
                python: "none".into(),
                sentence_transformers: "none".into(),
                transformers: "none".into(),
                torch: "none".into(),
                torchvision: "none".into(),
                pillow: "none".into(),
                torchcodec: "none".into(),
                ffmpeg: FfmpegIdentity {
                    version: "none".into(),
                    libavfilter: [0; 3],
                    libavutil: [0; 3],
                    libavcodec: [0; 3],
                    libavformat: [0; 3],
                },
            },
            processing: EffectiveProcessing {
                prompt_name: request.prompt_name,
                prompt_sha256: Sha256Digest::from_bytes([0; 32]),
                preprocessing: request.preprocessing.effective(),
            },
            items,
            usage: TokenUsage {
                input_tokens: tokens,
            },
        })
    }
}

/// Time, the synthetic Embedding and Time's Task worker on one mesh.
struct Mesh {
    running: RunningMesh,
    state: Arc<crate::state::TimeApplication>,
    db: TimeDb,
    _files: AuthorityFiles,
    _worker: tokio_util::sync::DropGuard,
}

impl Mesh {
    async fn start() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        // The replica's Task runtime on the mesh's deployment, whose calls act for a Task
        // only of their own deployment.
        let mut replica = (*files.replica(&db.a).await).clone();
        let slug = crate::changes::server();
        let instance = veoveo_store::Instance::start(&db.a, &slug).await.unwrap();
        replica.tasks = veoveo_task::TaskRuntime::new(
            db.a.clone(),
            slug,
            testing::deployment_id(),
            Arc::new(instance),
        );
        let state = Arc::new(replica);
        let mut mesh = TestMesh::bind().await;
        let calls = mesh.register_server(
            crate::changes::server(),
            <crate::mcp::TimeMcp as DomainServer>::routes().authorization(),
        );
        let handler = state.clone();
        let app = app_html();
        let served = app.clone();
        mesh.serve(
            testing::for_domain::<crate::mcp::TimeMcp>()
                .calls(calls.clone())
                .handler(move || {
                    Hosted::new(crate::mcp::TimeMcp::new(handler.clone(), served.clone()))
                        .with_tasks(DurableTasks::with_listener(
                            crate::server::tasks::TimeTaskExtension::new(handler.clone()),
                            crate::mcp::TimeSubscriptions::new(handler.clone()),
                        ))
                })
                .build(),
        );
        let synthetic = Synthetic::default();
        mesh.register_server(
            veoveo_types::ServerSlug::parse("embedding").unwrap(),
            Synthetic::routes().authorization(),
        );
        mesh.serve(
            testing::for_domain::<Synthetic>()
                .handler(move || Hosted::new(synthetic.clone()))
                .build(),
        );
        let running = mesh
            .granting(
                TimePermission::ALL
                    .iter()
                    .map(|permission| permission.name())
                    .chain([
                        PermissionName::parse(EmbeddingPermission::EmbeddingsCreate.as_str())
                            .unwrap(),
                    ]),
            )
            .start();
        let stop = CancellationToken::new();
        tokio::spawn(crate::server::tasks::work(
            state.clone(),
            Some(crate::server::tasks::Indexing {
                domain: crate::mcp::TimeMcp::new(state.clone(), app),
                calls,
            }),
            stop.clone(),
        ));
        Self {
            running,
            state,
            db,
            _files: files,
            _worker: stop.drop_guard(),
        }
    }

    async fn call(&self, tool: &str, arguments: Value) -> Value {
        let answer = self.running.call("time", tool, arguments).await;
        assert!(answer["error"].is_null(), "{tool}: {answer}");
        assert_ne!(answer["result"]["isError"], true, "{tool}: {answer}");
        answer
    }

    /// An instant an hour from now, as `resolve_time` answers it.
    async fn due(&self) -> Value {
        let request = ResolveTimeRequest {
            expression: TimeExpressionValue::Rfc3339 {
                value: (Utc::now() + chrono::TimeDelta::hours(1)).to_rfc3339(),
            }
            .build()
            .unwrap(),
            additional_uncertainty: crate::NanosecondBound::ZERO,
        };
        let answer = self
            .call("resolve_time", serde_json::to_value(request).unwrap())
            .await;
        answer["result"]["structuredContent"]["instant"].clone()
    }

    /// Create an event as the test user and return its ID and the Task the call named.
    async fn create(&self, name: &str) -> (String, String) {
        let due = self.due().await;
        let answer = self
            .call(
                "create_event",
                json!({"name": name, "due": due, "idempotencyKey": Uuid::now_v7().to_string()}),
            )
            .await;
        let event = answer["result"]["structuredContent"]["metadata"]["eventId"]
            .as_str()
            .or_else(|| answer["result"]["structuredContent"]["eventId"].as_str())
            .unwrap_or_else(|| panic!("{answer}"))
            .to_owned();
        let task = answer["result"]["_meta"][veoveo_hub_sdk::RELATED_TASK_META_KEY]["taskId"]
            .as_str()
            .unwrap_or_else(|| panic!("no index Task: {answer}"))
            .to_owned();
        (event, task)
    }

    /// The test user's view of Time's Task `task` once it settles.
    async fn settled(&self, task: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let answer = self.running.task("time", task).await;
                if answer["result"]["status"] != "working" {
                    return answer;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the index Task settles within 30 seconds")
    }

    /// The events a search answers, in rank order.
    async fn search(&self, parts: Value) -> Vec<String> {
        let answer = self.call("search_knowledge", json!({"parts": parts})).await;
        answer["result"]["structuredContent"]["hits"]
            .as_array()
            .unwrap_or_else(|| panic!("{answer}"))
            .iter()
            .map(|hit| hit["uri"].as_str().unwrap().to_owned())
            .collect()
    }
}

fn text(query: &str) -> Value {
    json!([{"type": "text", "text": query}])
}

fn uri(event: &str) -> String {
    format!("time://events/{event}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_event_is_found_by_its_words_at_once_and_by_its_meaning_after_its_index_task() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let mesh = Mesh::start().await;
        let (standup, task) = mesh.create("weekly standup review").await;
        // Words find it whether or not its vector is stored yet.
        assert_eq!(
            mesh.search(text("standup")).await.first(),
            Some(&uri(&standup))
        );

        let settled = mesh.settled(&task).await;
        assert_eq!(settled["result"]["status"], "completed", "{settled}");
        let model: Option<String> =
            sqlx::query_scalar("SELECT knowledge_model FROM time.event WHERE event_key = $1")
                .bind(&standup)
                .fetch_one(mesh.db.a.pool())
                .await
                .unwrap();
        assert_eq!(model.as_deref(), Some("synthetic/word-hash@1"));

        // Words that are not all in its name miss it as words, and find it by the
        // nearness of its vector once the Task stored one.
        let (launch, task) = mesh.create("rocket launch window").await;
        mesh.settled(&task).await;
        let found = mesh.search(text("standup retrospective")).await;
        assert_eq!(found.first(), Some(&uri(&standup)), "{found:?}");
        assert!(found.contains(&uri(&launch)), "{found:?}");
    })
    .await
    .expect("the test finished within 90 seconds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_image_query_on_text_events_answers_ranked_hits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let mesh = Mesh::start().await;
        let (first, task) = mesh.create("harbour inspection").await;
        mesh.settled(&task).await;
        let (second, task) = mesh.create("crew rotation").await;
        mesh.settled(&task).await;
        // The synthetic Embedding embeds the part by its media type and fetches nothing.
        let image = json!([{"type": "image",
            "url": "https://media.veoveo.test/synthetic.png?X-Amz-Signature=synthetic",
            "mediaType": "image/png", "size": 3}]);
        let found = mesh.search(image).await;
        let expected: std::collections::BTreeSet<_> = [uri(&first), uri(&second)].into();
        assert_eq!(
            found
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
    })
    .await
    .expect("the test finished within 90 seconds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_search_finds_only_the_callers_own_events() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let mesh = Mesh::start().await;
        // Another owner's event in the test user's tenant, found by the same words.
        let other = scope(&mesh.db.a, "tenant-a").await;
        let engine = mesh.state.engine(&other).await.unwrap();
        let due = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpressionValue::Rfc3339 {
                    value: (Utc::now() + chrono::TimeDelta::hours(1)).to_rfc3339(),
                }
                .build()
                .unwrap(),
                additional_uncertainty: crate::NanosecondBound::ZERO,
            })
            .unwrap()
            .into_instant();
        let utc = engine.utc_due(&due).unwrap();
        mesh.state
            .catalog
            .create_event(
                &other,
                Event {
                    event_id: EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap(),
                    name: "lantern festival".into(),
                    due,
                    state: EventState::Scheduled,
                    record_version: TimeVersion::FIRST,
                },
                utc,
                "other-owner".into(),
            )
            .await
            .unwrap();
        assert!(mesh.search(text("lantern")).await.is_empty());
        let (own, _) = mesh.create("lantern rehearsal").await;
        assert_eq!(mesh.search(text("lantern")).await, [uri(&own)]);
    })
    .await
    .expect("the test finished within 90 seconds");
}
