//! Acquisitions of authority releases. A repeated idempotency key returns the first
//! acquisition when the request matches it.
use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_acquisition(
        &self,
        draft: TimeAcquisitionDraft,
    ) -> Result<TimeAcquisitionRecord, PersistenceError> {
        validate_acquisition(&draft)?;
        let created = sqlx::query_file_as!(
            TimeAcquisitionRecord,
            "src/persistence/queries/insert_acquisition.sql",
            draft.identity.tenant_id as TenantId,
            &draft.acquisition_key as &TimeAcquisitionId,
            draft.identity.principal_id as PrincipalId,
            &draft.source_key as &TimeSourceId,
            draft
                .expected_source_digest
                .as_ref()
                .map(Sha256Digest::as_str),
            draft.idempotency_key,
            draft.status as TimeAcquisitionState,
            draft.phase as TimeAcquisitionPhase,
            draft.staged_release_key.as_ref() as Option<&AuthorityReleaseId>,
            draft.body,
        )
        .fetch_optional(&self.pool)
        .await?;
        if let Some(created) = created {
            return Ok(created);
        }
        let existing = self
            .time_acquisition_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id,
                &draft.idempotency_key,
            )
            .await?
            .ok_or_else(|| conflict("acquisition idempotency key", &draft.idempotency_key))?;
        let same_request = existing.source_key == draft.source_key
            && existing.expected_source_digest.as_deref()
                == draft
                    .expected_source_digest
                    .as_ref()
                    .map(Sha256Digest::as_str);
        if same_request {
            Ok(existing)
        } else {
            Err(conflict(
                "acquisition idempotency key",
                draft.idempotency_key,
            ))
        }
    }

    pub(crate) async fn time_acquisition(
        &self,
        tenant_id: TenantId,
        key: &TimeAcquisitionId,
    ) -> Result<Option<TimeAcquisitionRecord>, PersistenceError> {
        validate_key("acquisition_key", key, "time-acquisition-")?;
        Ok(sqlx::query_file_as!(
            TimeAcquisitionRecord,
            "src/persistence/queries/acquisition.sql",
            tenant_id as TenantId,
            key as &TimeAcquisitionId,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_acquisitions(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeAcquisitionRecord>, PersistenceError> {
        Ok(sqlx::query_file_as!(
            TimeAcquisitionRecord,
            "src/persistence/queries/acquisitions.sql",
            tenant_id as TenantId,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn time_acquisition_for_release(
        &self,
        tenant_id: TenantId,
        release_key: &AuthorityReleaseId,
    ) -> Result<Option<TimeAcquisitionRecord>, PersistenceError> {
        validate_key("staged_release_key", release_key, "time-release-")?;
        Ok(sqlx::query_file_as!(
            TimeAcquisitionRecord,
            "src/persistence/queries/acquisition_for_release.sql",
            tenant_id as TenantId,
            release_key as &AuthorityReleaseId,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn update_time_acquisition(
        &self,
        update: TimeAcquisitionUpdate,
    ) -> Result<TimeAcquisitionRecord, PersistenceError> {
        validate_key(
            "acquisition_key",
            &update.acquisition_key,
            "time-acquisition-",
        )?;
        validate_body(&update.body)?;
        if let Some(release) = &update.staged_release_key {
            validate_key("staged_release_key", release, "time-release-")?;
        }
        sqlx::query_file_as!(
            TimeAcquisitionRecord,
            "src/persistence/queries/update_acquisition.sql",
            update.tenant_id as TenantId,
            &update.acquisition_key as &TimeAcquisitionId,
            update.status as TimeAcquisitionState,
            update.phase as TimeAcquisitionPhase,
            update.staged_release_key.as_ref() as Option<&AuthorityReleaseId>,
            update.body,
            version(update.expected_record_version),
            next_version(update.expected_record_version)?,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| conflict("acquisition", &update.acquisition_key))
    }

    pub(crate) async fn time_acquisition_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: PrincipalId,
        key: &str,
    ) -> Result<Option<TimeAcquisitionRecord>, PersistenceError> {
        validate_text("idempotency_key", key, 256)?;
        Ok(sqlx::query_file_as!(
            TimeAcquisitionRecord,
            "src/persistence/queries/acquisition_by_idempotency.sql",
            tenant_id as TenantId,
            owner as PrincipalId,
            key,
        )
        .fetch_optional(&self.pool)
        .await?)
    }
}
