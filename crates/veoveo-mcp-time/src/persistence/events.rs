use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_event(
        &self,
        draft: TimeEventDraft,
    ) -> Result<TimeEventRecord, PersistenceError> {
        validate_event(&draft)?;
        if let Some(existing) = self
            .time_event_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id,
                &draft.idempotency_key,
            )
            .await?
        {
            if existing.name == draft.name
                && existing.due_tai_seconds_since_1970 == draft.due_tai_seconds_since_1970
                && existing.due_nanosecond == i64::from(draft.due_nanosecond.get())
            {
                return Ok(existing);
            }
            return Err(conflict("event idempotency key", draft.idempotency_key));
        }
        let now = Utc::now();
        let content = TimeEventContent {
            provenance: TimeProvenanceRecord::new(&draft.identity, &draft.work_context),
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            event_key: draft.event_key.to_string(),
            name: draft.name,
            state: draft.state,
            due_tai_seconds_since_1970: draft.due_tai_seconds_since_1970,
            due_nanosecond: i64::from(draft.due_nanosecond.get()),
            idempotency_key: draft.idempotency_key,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(self, time_record("time_event", &draft.event_key), content).await?;
        self.time_event(&draft.identity, &draft.event_key)
            .await?
            .ok_or(PersistenceError::MissingRecord {
                operation: "time event creation readback",
            })
    }

    pub(crate) async fn time_event(
        &self,
        identity: &PlatformIdentity,
        event_key: &EventId,
    ) -> Result<Option<TimeEventRecord>, PersistenceError> {
        validate_key("event_key", event_key, "event-")?;
        let mut response = self
            .client()
            .query(include_str!("queries/read_owned_event.surql"))
            .bind(("record", time_record("time_event", event_key)))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub(crate) async fn transition_time_event(
        &self,
        identity: &PlatformIdentity,
        event_key: &EventId,
        expected: TimeVersion,
        state: TimeEventState,
        canonical_json: String,
    ) -> Result<TimeEventRecord, PersistenceError> {
        validate_key("event_key", event_key, "event-")?;
        validate_json(&canonical_json)?;
        let mut response = self
            .client()
            .query(include_str!("queries/update_event.surql"))
            .bind(("record", time_record("time_event", event_key)))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("state", state))
            .bind(("canonical_json", canonical_json))
            .bind(("expected", expected.get() as i64))
            .bind(("next", expected.checked_next()?.get() as i64))
            .await?
            .check()?;
        response
            .take::<Option<TimeEventRecord>>(0)?
            .ok_or_else(|| conflict("event", event_key.to_string()))
    }

    async fn time_event_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: veoveo_platform_store::PrincipalId,
        key: &str,
    ) -> Result<Option<TimeEventRecord>, PersistenceError> {
        validate_text("idempotency_key", key, 256)?;
        let mut response = self
            .client()
            .query(include_str!("queries/event_by_idempotency.surql"))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("owner", owner.record_id()))
            .bind(("key", key.to_owned()))
            .await?
            .check()?;
        let records: Vec<TimeEventRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}
