//! Immutable calendar and epoch versions.
use super::*;

const MAX_CALENDAR_BYTES: usize = 64 * 1024;

impl TimePersistence {
    pub(crate) async fn create_time_calendar_version(
        &self,
        draft: TimeCalendarVersionDraft,
    ) -> Result<TimeCalendarVersionRecord, PersistenceError> {
        validate_key("calendar_key", &draft.calendar_key, "calendar-")?;
        validate_text("name", &draft.name, 256)?;
        validate_zone_id(&draft.zone_id)?;
        validate_body(&draft.body)?;
        if body_bytes(&draft.body) > MAX_CALENDAR_BYTES {
            return Err(invalid("calendar", "must fit the 64 KiB resource limit"));
        }
        sqlx::query_file_as!(
            TimeCalendarVersionRecord,
            "src/persistence/queries/insert_calendar_version.sql",
            draft.identity.tenant_id as TenantId,
            &draft.calendar_key as &CalendarId,
            version(draft.calendar_version),
            draft.identity.principal_id as PrincipalId,
            draft.name,
            draft.zone_id,
            draft.state as TimeCalendarState,
            draft.body,
            draft.identity.tenant_key,
            draft.identity.principal_key,
            draft.work_context.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            conflict(
                "calendar version",
                format!("{}@{}", draft.calendar_key, draft.calendar_version.get()),
            )
        })
    }

    pub(crate) async fn time_calendar_version(
        &self,
        tenant_id: TenantId,
        calendar_key: &CalendarId,
        calendar_version: TimeVersion,
    ) -> Result<Option<TimeCalendarVersionRecord>, PersistenceError> {
        validate_key("calendar_key", calendar_key, "calendar-")?;
        Ok(sqlx::query_file_as!(
            TimeCalendarVersionRecord,
            "src/persistence/queries/calendar_version.sql",
            tenant_id as TenantId,
            calendar_key as &CalendarId,
            version(calendar_version),
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn create_time_epoch(
        &self,
        draft: TimeEpochDraft,
    ) -> Result<TimeEpochRecord, PersistenceError> {
        validate_key("epoch_key", &draft.epoch_key, "epoch-")?;
        validate_text("name", &draft.name, 256)?;
        validate_body(&draft.body)?;
        sqlx::query_file_as!(
            TimeEpochRecord,
            "src/persistence/queries/insert_epoch.sql",
            draft.identity.tenant_id as TenantId,
            &draft.epoch_key as &EpochId,
            version(draft.epoch_version),
            draft.identity.principal_id as PrincipalId,
            draft.name,
            draft.tai_seconds_since_1970,
            nanosecond(draft.nanosecond),
            draft.body,
            draft.identity.tenant_key,
            draft.identity.principal_key,
            draft.work_context.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            conflict(
                "epoch version",
                format!("{}@{}", draft.epoch_key, draft.epoch_version.get()),
            )
        })
    }

    pub(crate) async fn time_epoch(
        &self,
        tenant_id: TenantId,
        epoch_key: &EpochId,
        epoch_version: TimeVersion,
    ) -> Result<Option<TimeEpochRecord>, PersistenceError> {
        validate_key("epoch_key", epoch_key, "epoch-")?;
        Ok(sqlx::query_file_as!(
            TimeEpochRecord,
            "src/persistence/queries/epoch_version.sql",
            tenant_id as TenantId,
            epoch_key as &EpochId,
            version(epoch_version),
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    /// The latest version of one tenant epoch.
    pub(crate) async fn latest_time_epoch(
        &self,
        tenant_id: TenantId,
        epoch_key: &EpochId,
    ) -> Result<Option<TimeEpochRecord>, PersistenceError> {
        Ok(self
            .latest_time_epochs(tenant_id, std::slice::from_ref(epoch_key))
            .await?
            .pop())
    }
}
