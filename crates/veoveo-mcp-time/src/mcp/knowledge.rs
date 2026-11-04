//! Time's knowledge search through the SDK's ready-made PostgreSQL search: the collection
//! `time.events`, a person's own events by their name, embedded through Embedding for
//! their owners.
use std::sync::LazyLock;

use rmcp::{RoleServer, service::RequestContext};
use sqlx::PgPool;
use veoveo_hub_sdk::{
    client::HubClient,
    hosting::{ToolError, hub_identity},
    knowledge::{
        ContentPart, ContentParts, KnowledgeCollection, KnowledgeHit, KnowledgeValueError,
        PgKnowledge, PgKnowledgeSource, PgKnowledgeTable, Snippet,
    },
};
use veoveo_mcp_embedding::client::EmbeddingClient;
use veoveo_types::ResourceUri;

use crate::contract::{EventId, EventState};

use super::{TimeMcp, internal};

/// Time's one searchable collection.
pub(crate) fn collection() -> KnowledgeCollection {
    KnowledgeCollection::parse("time.events").expect("Time's collection")
}

/// Time's search: its events, embedded through Embedding for their owners.
pub(crate) static KNOWLEDGE: LazyLock<PgKnowledge<TimeMcp, EmbeddingClient>> =
    LazyLock::new(|| PgKnowledge::new(EmbeddingClient).source(Events));

/// The events as a knowledge source: `time.event` by its `event_key`, under
/// `time.knowledge_readable`.
pub(crate) struct Events;

/// Why an event's content could not be given to embed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ContentError {
    #[error("the event could not be read: {0}")]
    Read(String),
    #[error("the event's name is not one Embedding takes: {0}")]
    Value(#[from] KnowledgeValueError),
}

impl PgKnowledgeSource<TimeMcp> for Events {
    type Key = String;
    type Error = ContentError;

    fn collection(&self) -> KnowledgeCollection {
        collection()
    }

    fn table(&self) -> PgKnowledgeTable {
        PgKnowledgeTable::new("event", "event_key", "knowledge_readable")
            .expect("Time's knowledge table")
    }

    fn pool<'a>(&self, domain: &'a TimeMcp) -> &'a PgPool {
        domain.state.store.pool()
    }

    async fn hits(
        &self,
        domain: &TimeMcp,
        request: &RequestContext<RoleServer>,
        keys: &[String],
    ) -> Result<Vec<Option<KnowledgeHit>>, ToolError> {
        let state = &domain.state;
        let scope = state
            .scope(&hub_identity(request)?)
            .await
            .map_err(internal)?;
        let engine = state.engine(&scope).await.map_err(internal)?;
        let mut hits = Vec::with_capacity(keys.len());
        for key in keys {
            let Ok(event_id) = EventId::parse(key.clone()) else {
                hits.push(None);
                continue;
            };
            // Gone, or not the caller's, since the ranking.
            let Some(event) = state
                .catalog
                .event(&scope, &event_id)
                .await
                .map_err(internal)?
            else {
                hits.push(None);
                continue;
            };
            let state_text = match event.state {
                EventState::Scheduled => "scheduled",
                EventState::Due => "due",
                EventState::Cancelled => "cancelled",
            };
            let snippet = match engine.utc_due(&event.due) {
                Ok(due) => format!("{state_text}, due {}", due.to_rfc3339()),
                Err(_) => state_text.to_owned(),
            };
            hits.push(Some(KnowledgeHit {
                uri: ResourceUri::new(crate::TimeResource::Event(event.event_id).to_string())
                    .map_err(internal)?,
                collection: collection(),
                title: event.name,
                snippet: Snippet::new(snippet),
                modified_at: None,
            }));
        }
        Ok(hits)
    }

    async fn content(
        &self,
        domain: &TimeMcp,
        _client: &HubClient,
        key: String,
    ) -> Result<Option<ContentParts>, ContentError> {
        let Ok(event_id) = EventId::parse(key) else {
            return Ok(None);
        };
        let name = domain
            .state
            .catalog
            .event_name(&event_id)
            .await
            .map_err(|error| ContentError::Read(error.to_string()))?;
        Ok(name
            .map(|name| ContentParts::one(ContentPart::text(name)))
            .transpose()?)
    }
}
