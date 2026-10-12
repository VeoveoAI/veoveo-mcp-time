//! Authority publication: one transaction locks the tenant's activations, rechecks the
//! preflight snapshot and candidate, and changes the candidate, the pointer and the
//! previous release together.
use super::*;

/// Only the catalog constructs a write from a checked candidate and active snapshot.
pub(crate) struct AuthorityActivation {
    pub(crate) candidate: TimeAuthorityReleaseRecord,
    pub(crate) snapshot: ActiveAuthoritySnapshot,
    pub(crate) expected_release: TimeVersion,
    pub(crate) expected_pointer: TimeWriteGuard,
    pub(crate) body: serde_json::Value,
}

/// Why an activation found the records changed. The transaction rolls back.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ActivationConflict {
    #[error("the active pointers or their releases changed after preflight")]
    Pair,
    #[error("the candidate release changed after preflight")]
    Candidate,
    #[error("the candidate is not the expected staged release version")]
    Release,
    #[error("the active pointer is not the expected version")]
    Pointer,
    #[error("the previous release is not the expected active version")]
    PreviousRelease,
}

impl TimePersistence {
    pub(crate) async fn commit_time_authority_release(
        &self,
        identity: &PlatformIdentity,
        activation: AuthorityActivation,
    ) -> Result<TimeAuthorityReleaseRecord, PersistenceError> {
        let AuthorityActivation {
            candidate,
            snapshot,
            expected_release,
            expected_pointer,
            body,
        } = activation;
        let tenant = identity.tenant_id;
        if snapshot.tenant != tenant {
            return Err(invalid(
                "activation.tenant",
                "snapshot belongs to another tenant",
            ));
        }
        let release_key = candidate.release_key.clone();
        validate_key("release_key", &release_key, "time-release-")?;
        validate_body(&body)?;
        let next_release = next_version(expected_release)?;
        let next_pointer = version(expected_pointer.next_version()?);
        let rejected = |conflict| PersistenceError::ActivationConflict {
            release_key: release_key.clone(),
            conflict,
        };
        if candidate.record_version != version(expected_release)
            || candidate.state != TimeAuthorityReleaseState::Staged
            || candidate.tenant != tenant
        {
            return Err(rejected(ActivationConflict::Release));
        }
        let kind = candidate.dataset_kind;
        let pointer = snapshot.pointer(kind)?;
        let observed_guard = pointer.as_ref().map_or(TimeWriteGuard::Absent, |pointer| {
            TimeWriteGuard::Existing(pointer.record_version)
        });
        if observed_guard != expected_pointer {
            return Err(rejected(ActivationConflict::Pointer));
        }
        let mut locked_releases = snapshot
            .releases()
            .map(|release| release.release_key)
            .chain(std::iter::once(release_key.clone()))
            .collect::<Vec<_>>();
        locked_releases.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        locked_releases.dedup();

        let mut transaction = self.pool.begin().await?;
        sqlx::query_file_scalar!(
            "src/persistence/queries/lock_tenant_activation.sql",
            tenant as TenantId,
        )
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query_file_as!(
            TimeActiveAuthorityRecord,
            "src/persistence/queries/lock_pointers.sql",
            tenant as TenantId,
        )
        .fetch_all(&mut *transaction)
        .await?;
        sqlx::query_file_scalar!(
            "src/persistence/queries/lock_releases.sql",
            tenant as TenantId,
            &locked_releases as &[AuthorityReleaseId],
        )
        .fetch_all(&mut *transaction)
        .await?;
        if active::read_snapshot(&mut transaction, tenant).await? != snapshot {
            return Err(rejected(ActivationConflict::Pair));
        }
        let current = sqlx::query_file_as!(
            TimeAuthorityReleaseRecord,
            "src/persistence/queries/release.sql",
            tenant as TenantId,
            &release_key as &AuthorityReleaseId,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        if current.as_ref() != Some(&candidate) {
            return Err(rejected(ActivationConflict::Candidate));
        }
        let activated = sqlx::query_file_as!(
            TimeAuthorityReleaseRecord,
            "src/persistence/queries/activate_release.sql",
            tenant as TenantId,
            &release_key as &AuthorityReleaseId,
            kind as TimeDatasetKind,
            version(expected_release),
            body,
            next_release,
        )
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| rejected(ActivationConflict::Release))?;
        match &pointer {
            None => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/insert_pointer.sql",
                    tenant as TenantId,
                    kind as TimeDatasetKind,
                    &release_key as &AuthorityReleaseId,
                    identity.principal_id as PrincipalId,
                )
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| rejected(ActivationConflict::Pointer))?;
            }
            Some(previous) => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/replace_pointer.sql",
                    tenant as TenantId,
                    kind as TimeDatasetKind,
                    &release_key as &AuthorityReleaseId,
                    &previous.release_key as &AuthorityReleaseId,
                    identity.principal_id as PrincipalId,
                    version(previous.record_version),
                    next_pointer,
                    previous.previous_release_key.as_ref() as Option<&AuthorityReleaseId>,
                )
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| rejected(ActivationConflict::Pointer))?;
                sqlx::query_file_scalar!(
                    "src/persistence/queries/retire_release.sql",
                    tenant as TenantId,
                    kind as TimeDatasetKind,
                    &previous.release_key as &AuthorityReleaseId,
                    previous.release.record_version,
                )
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| rejected(ActivationConflict::PreviousRelease))?;
            }
        }
        transaction.commit().await?;
        Ok(activated)
    }
}

#[cfg(test)]
impl TimePersistence {
    pub(super) async fn activate_time_authority_release(
        &self,
        identity: &PlatformIdentity,
        release_key: &AuthorityReleaseId,
        expected_release: TimeVersion,
        expected_pointer: TimeWriteGuard,
        body: serde_json::Value,
    ) -> Result<TimeAuthorityReleaseRecord, PersistenceError> {
        let candidate = self
            .time_authority_release(identity.tenant_id, release_key)
            .await?
            .ok_or_else(|| conflict("authority release", release_key))?;
        let snapshot = self.active_authority_snapshot(identity.tenant_id).await?;
        self.commit_time_authority_release(
            identity,
            AuthorityActivation {
                candidate,
                snapshot,
                expected_release,
                expected_pointer,
                body,
            },
        )
        .await
    }
}
