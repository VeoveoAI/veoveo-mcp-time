//! Events their owners create and cancel. The due loop in `due.rs` moves scheduled
//! events to due.
use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_event(
        &self,
        draft: TimeEventDraft,
    ) -> Result<TimeEventRecord, PersistenceError> {
        validate_event(&draft)?;
        let created = sqlx::query_file_as!(
            TimeEventRecord,
            "src/persistence/queries/insert_event.sql",
            draft.identity.tenant_id as TenantId,
            &draft.event_key as &EventId,
            draft.identity.principal_id as PrincipalId,
            draft.name,
            draft.state as TimeEventState,
            draft.due_tai_seconds_since_1970,
            nanosecond(draft.due_nanosecond),
            draft.due,
            draft.idempotency_key,
            draft.body,
            draft.identity.tenant_key,
            draft.identity.principal_key,
            draft.work_context.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;
        if let Some(created) = created {
            return Ok(created);
        }
        let existing = self
            .time_event_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id,
                &draft.idempotency_key,
            )
            .await?
            .ok_or_else(|| conflict("event idempotency key", &draft.idempotency_key))?;
        let same_request = existing.name == draft.name
            && existing.due_tai_seconds_since_1970 == draft.due_tai_seconds_since_1970
            && existing.due_nanosecond == nanosecond(draft.due_nanosecond);
        if same_request {
            Ok(existing)
        } else {
            Err(conflict("event idempotency key", draft.idempotency_key))
        }
    }

    pub(crate) async fn time_event(
        &self,
        identity: &PlatformIdentity,
        event_key: &EventId,
    ) -> Result<Option<TimeEventRecord>, PersistenceError> {
        validate_key("event_key", event_key, "event-")?;
        Ok(sqlx::query_file_as!(
            TimeEventRecord,
            "src/persistence/queries/event.sql",
            identity.tenant_id as TenantId,
            event_key as &EventId,
            identity.principal_id as PrincipalId,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn transition_time_event(
        &self,
        identity: &PlatformIdentity,
        event_key: &EventId,
        expected: TimeVersion,
        state: TimeEventState,
        body: serde_json::Value,
    ) -> Result<TimeEventRecord, PersistenceError> {
        validate_key("event_key", event_key, "event-")?;
        validate_body(&body)?;
        sqlx::query_file_as!(
            TimeEventRecord,
            "src/persistence/queries/update_event_state.sql",
            identity.tenant_id as TenantId,
            event_key as &EventId,
            identity.principal_id as PrincipalId,
            state as TimeEventState,
            body,
            version(expected),
            next_version(expected)?,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| conflict("event", event_key))
    }

    async fn time_event_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: PrincipalId,
        key: &str,
    ) -> Result<Option<TimeEventRecord>, PersistenceError> {
        validate_text("idempotency_key", key, 256)?;
        Ok(sqlx::query_file_as!(
            TimeEventRecord,
            "src/persistence/queries/event_by_idempotency.sql",
            tenant_id as TenantId,
            owner as PrincipalId,
            key,
        )
        .fetch_optional(&self.pool)
        .await?)
    }
}
