//! A tenant's clock quality policy.
use super::*;

impl TimePersistence {
    pub(crate) async fn replace_time_clock_policy(
        &self,
        draft: TimeClockPolicyDraft,
        expected: TimeWriteGuard,
    ) -> Result<TimeClockPolicyRecord, PersistenceError> {
        let next = expected.next_version()?;
        let tenant = draft.identity.tenant_id;
        let policy = &draft.policy;
        let maximum_error = i64::try_from(policy.maximum_error_nanoseconds())
            .map_err(|_| invalid("maximum_error_nanoseconds", "exceeds the database range"))?;
        let maximum_holdover = i64::try_from(policy.maximum_holdover_seconds())
            .map_err(|_| invalid("maximum_holdover_seconds", "exceeds the database range"))?;
        let record = match expected {
            TimeWriteGuard::Absent => {
                sqlx::query_file_as!(
                    TimeClockPolicyRecord,
                    "src/persistence/queries/insert_clock_policy.sql",
                    tenant as TenantId,
                    draft.identity.principal_id as PrincipalId,
                    maximum_error,
                    i16::from(policy.maximum_stratum()),
                    i64::from(policy.minimum_source_diversity()),
                    maximum_holdover,
                )
                .fetch_optional(&self.pool)
                .await?
            }
            TimeWriteGuard::Existing(current) => {
                sqlx::query_file_as!(
                    TimeClockPolicyRecord,
                    "src/persistence/queries/update_clock_policy.sql",
                    tenant as TenantId,
                    draft.identity.principal_id as PrincipalId,
                    maximum_error,
                    i16::from(policy.maximum_stratum()),
                    i64::from(policy.minimum_source_diversity()),
                    maximum_holdover,
                    version(current),
                    version(next),
                )
                .fetch_optional(&self.pool)
                .await?
            }
        };
        record.ok_or_else(|| conflict("clock policy", tenant))
    }

    pub(crate) async fn time_clock_policy(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<TimeClockPolicyRecord>, PersistenceError> {
        Ok(sqlx::query_file_as!(
            TimeClockPolicyRecord,
            "src/persistence/queries/clock_policy.sql",
            tenant_id as TenantId,
        )
        .fetch_optional(&self.pool)
        .await?)
    }
}
