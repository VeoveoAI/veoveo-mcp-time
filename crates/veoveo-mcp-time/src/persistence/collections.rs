//! Keyset pages over each collection, in the order its cursor encodes. Every page has a
//! first-page and a next-page statement, so no predicate depends on whether a cursor is
//! present, and a cached generic plan still seeks the index past the cursor.
use super::*;
use crate::contract::{
    AuthorityReleaseId, CalendarPosition, EpochPosition, EventPosition, TimeAcquisitionId,
    TimeSourceId,
};

fn validate_limit(limit: u32) -> Result<i64, PersistenceError> {
    if !(1..=101).contains(&limit) {
        return Err(invalid("limit", "must be in 1..=101"));
    }
    Ok(i64::from(limit))
}

impl TimePersistence {
    pub(crate) async fn time_sources_page(
        &self,
        tenant: TenantId,
        after: Option<&TimeSourceId>,
        limit: u32,
    ) -> Result<Vec<TimeSourceRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        let Some(after) = after else {
            return Ok(sqlx::query_file_as!(
                TimeSourceRecord,
                "src/persistence/queries/source_page_first.sql",
                tenant as TenantId,
                limit,
            )
            .fetch_all(&self.pool)
            .await?);
        };
        validate_key("source_key", after, "time-source-")?;
        Ok(sqlx::query_file_as!(
            TimeSourceRecord,
            "src/persistence/queries/source_page_after.sql",
            tenant as TenantId,
            after as &TimeSourceId,
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn time_acquisitions_page(
        &self,
        tenant: TenantId,
        after: Option<&TimeAcquisitionId>,
        limit: u32,
    ) -> Result<Vec<TimeAcquisitionRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        let Some(after) = after else {
            return Ok(sqlx::query_file_as!(
                TimeAcquisitionRecord,
                "src/persistence/queries/acquisition_page_first.sql",
                tenant as TenantId,
                limit,
            )
            .fetch_all(&self.pool)
            .await?);
        };
        validate_key("acquisition_key", after, "time-acquisition-")?;
        Ok(sqlx::query_file_as!(
            TimeAcquisitionRecord,
            "src/persistence/queries/acquisition_page_after.sql",
            tenant as TenantId,
            after as &TimeAcquisitionId,
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn time_authority_releases_page(
        &self,
        tenant: TenantId,
        after: Option<&AuthorityReleaseId>,
        limit: u32,
    ) -> Result<Vec<TimeAuthorityReleaseRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        let Some(after) = after else {
            return Ok(sqlx::query_file_as!(
                TimeAuthorityReleaseRecord,
                "src/persistence/queries/release_page_first.sql",
                tenant as TenantId,
                limit,
            )
            .fetch_all(&self.pool)
            .await?);
        };
        Ok(sqlx::query_file_as!(
            TimeAuthorityReleaseRecord,
            "src/persistence/queries/release_page_after.sql",
            tenant as TenantId,
            after as &AuthorityReleaseId,
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_calendar_versions(
        &self,
        tenant_id: TenantId,
        after: Option<&CalendarPosition>,
        limit: u32,
    ) -> Result<Vec<TimeCalendarVersionRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        let Some(after) = after else {
            return Ok(sqlx::query_file_as!(
                TimeCalendarVersionRecord,
                "src/persistence/queries/calendar_page_first.sql",
                tenant_id as TenantId,
                limit,
            )
            .fetch_all(&self.pool)
            .await?);
        };
        validate_key("calendar_key", &after.calendar_id, "calendar-")?;
        Ok(sqlx::query_file_as!(
            TimeCalendarVersionRecord,
            "src/persistence/queries/calendar_page_after.sql",
            tenant_id as TenantId,
            &after.calendar_id as &CalendarId,
            version(after.version),
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_epochs(
        &self,
        tenant_id: TenantId,
        after: Option<&EpochPosition>,
        limit: u32,
    ) -> Result<Vec<TimeEpochRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        let Some(after) = after else {
            return Ok(sqlx::query_file_as!(
                TimeEpochRecord,
                "src/persistence/queries/epoch_page_first.sql",
                tenant_id as TenantId,
                limit,
            )
            .fetch_all(&self.pool)
            .await?);
        };
        validate_key("epoch_key", &after.epoch_id, "epoch-")?;
        Ok(sqlx::query_file_as!(
            TimeEpochRecord,
            "src/persistence/queries/epoch_page_after.sql",
            tenant_id as TenantId,
            &after.epoch_id as &EpochId,
            version(after.version),
            limit,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_events(
        &self,
        identity: &PlatformIdentity,
        after: Option<&EventPosition>,
        state: Option<TimeEventState>,
        limit: u32,
    ) -> Result<Vec<TimeEventRecord>, PersistenceError> {
        let limit = validate_limit(limit)?;
        let Some(after) = after else {
            return Ok(sqlx::query_file_as!(
                TimeEventRecord,
                "src/persistence/queries/event_page_first.sql",
                identity.tenant_id as TenantId,
                identity.principal_id as PrincipalId,
                state as Option<TimeEventState>,
                limit,
            )
            .fetch_all(&self.pool)
            .await?);
        };
        validate_key("event_key", &after.event_id, "event-")?;
        Ok(sqlx::query_file_as!(
            TimeEventRecord,
            "src/persistence/queries/event_page_after.sql",
            identity.tenant_id as TenantId,
            identity.principal_id as PrincipalId,
            state as Option<TimeEventState>,
            after.tai_seconds,
            nanosecond(after.nanosecond),
            &after.event_id as &EventId,
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
