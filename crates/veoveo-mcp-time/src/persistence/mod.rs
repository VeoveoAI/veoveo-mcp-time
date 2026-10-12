//! Time's tables in its PostgreSQL schema `time`. Every statement is a query file that
//! sqlx checks against the migrated schema at compile time.

use crate::catalog::TimeCompletion;
use crate::contract::{
    AuthorityReleaseId, CalendarId, ClockQualityPolicy, EpochId, EventId, SubsecondNanoseconds,
    TimeAcquisitionId, TimeAcquisitionPhase, TimeSourceId, TimeVersion, TimeVersionError,
    TimeWriteGuard,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use veoveo_platform_store::{PlatformIdentity, PrincipalId, TenantId};
use veoveo_types::Sha256Digest;

mod acquisitions;
mod activation;
mod active;
mod authority;
mod calendars;
mod clock;
mod collections;
mod completion;
mod drafts;
mod due;
mod events;
mod provenance;
mod records;
mod validation;

pub(crate) use activation::AuthorityActivation;
pub(crate) use active::ActiveAuthoritySnapshot;
pub(crate) use drafts::*;
pub(crate) use due::{DueEvent, NextDue};
pub(crate) use records::*;
pub(crate) use validation::validate_key;
use validation::*;

/// Time's migrations in `migrations/`. The server applies them in its schema under its
/// group role; their history lives in `time._sqlx_migrations`.
pub(crate) fn migrator() -> sqlx::migrate::Migrator {
    sqlx::migrate!()
}

#[derive(Clone)]
pub(crate) struct TimePersistence {
    pool: PgPool,
}

impl TimePersistence {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PersistenceError {
    #[error(transparent)]
    Version(#[from] TimeVersionError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("invalid time field {field}: {reason}")]
    InvalidTimeField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("time {entity} `{key}` conflicts with the current durable record")]
    TimeRecordConflict { entity: &'static str, key: String },
    #[error(
        "time authority activation of `{release_key}` conflicts with the current records: {conflict}"
    )]
    ActivationConflict {
        release_key: AuthorityReleaseId,
        conflict: activation::ActivationConflict,
    },
}

fn invalid(field: &'static str, reason: &'static str) -> PersistenceError {
    PersistenceError::InvalidTimeField { field, reason }
}

fn conflict(entity: &'static str, key: impl ToString) -> PersistenceError {
    PersistenceError::TimeRecordConflict {
        entity,
        key: key.to_string(),
    }
}

/// A stored version as the database's signed integer.
fn version(value: TimeVersion) -> i64 {
    i64::try_from(value.get()).expect("TimeVersion admits only the signed database range")
}

/// The next stored version after `value`, or a version error when it is exhausted.
fn next_version(value: TimeVersion) -> Result<i64, PersistenceError> {
    Ok(version(value.checked_next()?))
}

/// A subsecond coordinate as the database's `integer`.
fn nanosecond(value: SubsecondNanoseconds) -> i32 {
    i32::try_from(value.get()).expect("subsecond nanoseconds are below one billion")
}

#[cfg(test)]
mod activation_tests;
