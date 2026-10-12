//! Authority sources and the releases acquired from them.
use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_source(
        &self,
        draft: TimeSourceDraft,
    ) -> Result<TimeSourceRecord, PersistenceError> {
        validate_source(&draft)?;
        sqlx::query_file_as!(
            TimeSourceRecord,
            "src/persistence/queries/insert_source.sql",
            draft.identity.tenant_id as TenantId,
            &draft.source_key as &TimeSourceId,
            draft.identity.principal_id as PrincipalId,
            draft.name,
            draft.dataset_kind as TimeDatasetKind,
            draft.source_url,
            draft.expected_content_type,
            draft.enabled,
            draft.body,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| conflict("source", &draft.source_key))
    }

    pub(crate) async fn replace_time_source(
        &self,
        draft: TimeSourceDraft,
        expected_record_version: TimeVersion,
    ) -> Result<TimeSourceRecord, PersistenceError> {
        validate_source(&draft)?;
        sqlx::query_file_as!(
            TimeSourceRecord,
            "src/persistence/queries/update_source.sql",
            draft.identity.tenant_id as TenantId,
            &draft.source_key as &TimeSourceId,
            draft.name,
            draft.dataset_kind as TimeDatasetKind,
            draft.source_url,
            draft.expected_content_type,
            draft.enabled,
            draft.body,
            version(expected_record_version),
            next_version(expected_record_version)?,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| conflict("source", &draft.source_key))
    }

    pub(crate) async fn time_source(
        &self,
        tenant_id: TenantId,
        source_key: &TimeSourceId,
    ) -> Result<Option<TimeSourceRecord>, PersistenceError> {
        validate_key("source_key", source_key, "time-source-")?;
        Ok(sqlx::query_file_as!(
            TimeSourceRecord,
            "src/persistence/queries/source.sql",
            tenant_id as TenantId,
            source_key as &TimeSourceId,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_sources(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeSourceRecord>, PersistenceError> {
        Ok(sqlx::query_file_as!(
            TimeSourceRecord,
            "src/persistence/queries/sources.sql",
            tenant_id as TenantId,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn create_time_authority_release(
        &self,
        draft: TimeAuthorityReleaseDraft,
    ) -> Result<TimeAuthorityReleaseRecord, PersistenceError> {
        validate_release(&draft)?;
        sqlx::query_file_as!(
            TimeAuthorityReleaseRecord,
            "src/persistence/queries/insert_release.sql",
            draft.identity.tenant_id as TenantId,
            &draft.release_key as &AuthorityReleaseId,
            draft.identity.principal_id as PrincipalId,
            &draft.source_key as &TimeSourceId,
            draft.dataset_kind as TimeDatasetKind,
            draft.state as TimeAuthorityReleaseState,
            draft.version_label,
            draft.source_url,
            draft.source_digest.as_str(),
            draft.artifact_path,
            draft.retrieved_at,
            draft.validated_at,
            draft.body,
            draft.identity.tenant_key,
            draft.identity.principal_key,
            draft.work_context.as_str(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| release_conflict(error, &draft.release_key))?
        .ok_or_else(|| conflict("authority release", &draft.release_key))
    }

    pub(crate) async fn time_authority_release(
        &self,
        tenant_id: TenantId,
        release_key: &AuthorityReleaseId,
    ) -> Result<Option<TimeAuthorityReleaseRecord>, PersistenceError> {
        validate_key("release_key", release_key, "time-release-")?;
        Ok(sqlx::query_file_as!(
            TimeAuthorityReleaseRecord,
            "src/persistence/queries/release.sql",
            tenant_id as TenantId,
            release_key as &AuthorityReleaseId,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn list_time_authority_releases(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeAuthorityReleaseRecord>, PersistenceError> {
        Ok(sqlx::query_file_as!(
            TimeAuthorityReleaseRecord,
            "src/persistence/queries/releases.sql",
            tenant_id as TenantId,
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

/// A second release of the same source bytes conflicts with the first.
fn release_conflict(error: sqlx::Error, release_key: &AuthorityReleaseId) -> PersistenceError {
    match error.as_database_error() {
        Some(database) if database.is_unique_violation() => {
            conflict("authority release", release_key)
        }
        _ => error.into(),
    }
}
