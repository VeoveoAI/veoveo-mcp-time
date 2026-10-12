//! Completion candidates: distinct stored values that contain the typed needle.
use super::*;

impl TimePersistence {
    pub(crate) async fn complete_time_values(
        &self,
        identity: &PlatformIdentity,
        domain: TimeCompletion,
        needle: &str,
        limit: u32,
    ) -> Result<Vec<String>, PersistenceError> {
        if !(1..=101).contains(&limit) {
            return Err(invalid("limit", "must be in 1..=101"));
        }
        let limit = i64::from(limit);
        let needle = needle.to_lowercase();
        let tenant = identity.tenant_id;
        let candidates = match domain {
            TimeCompletion::CalendarId => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_calendar_id.sql",
                    tenant as TenantId,
                    needle,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::CalendarVersion { calendar_key } => {
                if let Some(key) = calendar_key.as_ref() {
                    validate_key("calendar_key", key, "calendar-")?;
                }
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_calendar_version.sql",
                    tenant as TenantId,
                    calendar_key.as_ref() as Option<&CalendarId>,
                    needle,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::EpochId => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_epoch_id.sql",
                    tenant as TenantId,
                    needle,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::EpochVersion { epoch_key } => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_epoch_version.sql",
                    tenant as TenantId,
                    epoch_key.as_ref() as Option<&EpochId>,
                    needle,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::AuthorityReleaseId => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_release_id.sql",
                    tenant as TenantId,
                    needle,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::EventId => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_event_id.sql",
                    tenant as TenantId,
                    identity.principal_id as PrincipalId,
                    needle,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(candidates)
    }
}
