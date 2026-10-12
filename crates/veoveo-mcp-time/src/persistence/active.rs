//! A tenant's active pointers and the releases they select, read from one snapshot.
use super::*;
use sqlx::PgConnection;

#[cfg(test)]
#[path = "active_tests.rs"]
mod tests;

/// The pointers and their releases as SQL selected them, before loading authority files.
/// A pointer whose release is missing here references a release that is not active or
/// not of its family.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActiveAuthoritySnapshot {
    pub(super) tenant: TenantId,
    pub(super) pointers: Vec<TimeActiveAuthorityRecord>,
    pub(super) releases: Vec<TimeAuthorityReleaseRecord>,
}

impl ActiveAuthoritySnapshot {
    /// The selected releases; admission guarantees one per pointer.
    pub(crate) fn releases(&self) -> impl Iterator<Item = TimeAuthorityReleaseRecord> + '_ {
        self.releases.iter().cloned()
    }

    pub(super) fn pointer(
        &self,
        kind: TimeDatasetKind,
    ) -> Result<Option<ActiveAuthority>, PersistenceError> {
        self.pointers
            .iter()
            .find(|pointer| pointer.dataset_kind == kind)
            .map(|pointer| self.admit(pointer))
            .transpose()
    }

    fn admitted(&self) -> Result<Vec<ActiveAuthority>, PersistenceError> {
        self.pointers
            .iter()
            .map(|pointer| self.admit(pointer))
            .collect()
    }

    fn admit(
        &self,
        pointer: &TimeActiveAuthorityRecord,
    ) -> Result<ActiveAuthority, PersistenceError> {
        if pointer.tenant != self.tenant {
            return Err(invalid(
                "active_authority.tenant",
                "pointer belongs to another tenant",
            ));
        }
        let record_version = u64::try_from(pointer.record_version)
            .ok()
            .and_then(|value| TimeVersion::new(value).ok())
            .ok_or_else(|| {
                invalid(
                    "active_authority.record_version",
                    "must be a positive stored version",
                )
            })?;
        validate_key(
            "active_authority.release_key",
            &pointer.release_key,
            "time-release-",
        )?;
        if let Some(previous) = &pointer.previous_release_key {
            validate_key(
                "active_authority.previous_release_key",
                previous,
                "time-release-",
            )?;
        }
        if pointer.previous_release_key.as_ref() == Some(&pointer.release_key)
            || (record_version == TimeVersion::FIRST) != pointer.previous_release_key.is_none()
        {
            return Err(invalid(
                "active_authority.previous_release_key",
                "must agree with pointer history",
            ));
        }
        // SQL excludes a release of another family or one that is not active. Its absence
        // is corruption of a visible pointer, not an absent pointer eligible for bootstrap
        // authority.
        let release = self
            .releases
            .iter()
            .find(|release| {
                release.tenant == self.tenant
                    && release.dataset_kind == pointer.dataset_kind
                    && release.release_key == pointer.release_key
                    && release.state == TimeAuthorityReleaseState::Active
            })
            .cloned()
            .ok_or_else(|| {
                invalid(
                    "active_authority.release",
                    "must reference an active release of the same tenant and family",
                )
            })?;
        validate_positive(
            "active_authority.release.record_version",
            release.record_version,
        )?;
        Ok(ActiveAuthority {
            release_key: pointer.release_key.clone(),
            previous_release_key: pointer.previous_release_key.clone(),
            record_version,
            release,
        })
    }
}

/// Admitted pointer metadata; the catalog separately admits the release's JSON body.
#[derive(Debug)]
pub(crate) struct ActiveAuthority {
    pub(crate) release_key: AuthorityReleaseId,
    pub(crate) previous_release_key: Option<AuthorityReleaseId>,
    pub(crate) record_version: TimeVersion,
    pub(crate) release: TimeAuthorityReleaseRecord,
}

impl TimePersistence {
    /// Read the pointers and their releases in one repeatable-read snapshot and admit
    /// every pointer.
    pub(crate) async fn active_authority_snapshot(
        &self,
        tenant: TenantId,
    ) -> Result<ActiveAuthoritySnapshot, PersistenceError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .await?;
        let snapshot = read_snapshot(&mut transaction, tenant).await?;
        transaction.commit().await?;
        snapshot.admitted()?;
        Ok(snapshot)
    }

    #[cfg(test)]
    pub(crate) async fn active_time_authority(
        &self,
        tenant: TenantId,
        kind: TimeDatasetKind,
    ) -> Result<Option<ActiveAuthority>, PersistenceError> {
        self.active_authority_snapshot(tenant).await?.pointer(kind)
    }

    pub(crate) async fn list_active_time_authorities(
        &self,
        tenant: TenantId,
    ) -> Result<Vec<ActiveAuthority>, PersistenceError> {
        self.active_authority_snapshot(tenant).await?.admitted()
    }
}

/// The pointers and the active releases of their families, on one connection.
pub(super) async fn read_snapshot(
    connection: &mut PgConnection,
    tenant: TenantId,
) -> Result<ActiveAuthoritySnapshot, PersistenceError> {
    let pointers = sqlx::query_file_as!(
        TimeActiveAuthorityRecord,
        "src/persistence/queries/active_pointers.sql",
        tenant as TenantId,
    )
    .fetch_all(&mut *connection)
    .await?;
    let releases = sqlx::query_file_as!(
        TimeAuthorityReleaseRecord,
        "src/persistence/queries/active_releases.sql",
        tenant as TenantId,
    )
    .fetch_all(&mut *connection)
    .await?;
    Ok(ActiveAuthoritySnapshot {
        tenant,
        pointers,
        releases,
    })
}
