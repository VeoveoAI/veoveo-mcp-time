//! Keyset pages over each collection, in the order its cursor encodes.
use super::*;
use crate::contract::{AuthorityCursor, CalendarCursor, EpochCursor, EventCursor};

fn validate_limit(limit: u32) -> Result<i64, PersistenceError> {
    if !(1..=101).contains(&limit) {
        return Err(invalid("limit", "must be in 1..=101"));
    }
    Ok(i64::from(limit))
}

impl TimePersistence {
    pub(crate) async fn time_authority_releases_page(
        &self,
        tenant: TenantId,
        after: Option<&AuthorityCursor>,
        limit: u32,
    ) -> Result<Vec<TimeAuthorityReleaseRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        Ok(sqlx::query_file_as!(
            TimeAuthorityReleaseRecord,
            "src/persistence/queries/release_page.sql",
            tenant as TenantId,
            after.map(AuthorityCursor::release_id) as Option<&AuthorityReleaseId>,
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_calendar_versions(
        &self,
        tenant_id: TenantId,
        after: Option<&CalendarCursor>,
        limit: u32,
    ) -> Result<Vec<TimeCalendarVersionRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        if let Some(after) = after {
            validate_key("calendar_key", after.calendar_id(), "calendar-")?;
        }
        Ok(sqlx::query_file_as!(
            TimeCalendarVersionRecord,
            "src/persistence/queries/calendar_page.sql",
            tenant_id as TenantId,
            after.map(CalendarCursor::calendar_id) as Option<&CalendarId>,
            after.map(|after| version(after.version())),
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_epochs(
        &self,
        tenant_id: TenantId,
        after: Option<&EpochCursor>,
        limit: u32,
    ) -> Result<Vec<TimeEpochRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        if let Some(after) = after {
            validate_key("epoch_key", after.epoch_id(), "epoch-")?;
        }
        Ok(sqlx::query_file_as!(
            TimeEpochRecord,
            "src/persistence/queries/epoch_page.sql",
            tenant_id as TenantId,
            after.map(EpochCursor::epoch_id) as Option<&EpochId>,
            after.map(|after| version(after.version())),
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_events(
        &self,
        identity: &PlatformIdentity,
        after: Option<&EventCursor>,
        state: Option<TimeEventState>,
        limit: u32,
    ) -> Result<Vec<TimeEventRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        if let Some(after) = after {
            validate_key("event_key", after.event_id(), "event-")?;
        }
        Ok(sqlx::query_file_as!(
            TimeEventRecord,
            "src/persistence/queries/event_page.sql",
            identity.tenant_id as TenantId,
            identity.principal_id as PrincipalId,
            state as Option<TimeEventState>,
            after.map(EventCursor::tai_seconds),
            after.map(|after| nanosecond(after.nanosecond())),
            after.map(EventCursor::event_id) as Option<&EventId>,
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// The latest versions of up to 100 requested epochs.
    pub(crate) async fn latest_time_epochs(
        &self,
        tenant_id: TenantId,
        keys: &[EpochId],
    ) -> Result<Vec<TimeEpochRecord>, PersistenceError> {
        if keys.len() > 100 {
            return Err(invalid("epoch_keys", "must contain at most 100 keys"));
        }
        for key in keys {
            validate_key("epoch_key", key, "epoch-")?;
        }
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        Ok(sqlx::query_file_as!(
            TimeEpochRecord,
            "src/persistence/queries/latest_epochs.sql",
            tenant_id as TenantId,
            keys as &[EpochId],
        )
        .fetch_all(&self.pool)
        .await?)
    }
}
