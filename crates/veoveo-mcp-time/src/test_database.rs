//! A fresh database per test with the kernel migrations and Time's own, two pools
//! connected as Time's server logins, as two replicas would be, and the corruption
//! queries that tests use to bypass catalog admission.
use veoveo_store::{PlatformStore, TenantId};
use veoveo_test_fixtures::store::TestDb;
use veoveo_types::ServerSlug;

use crate::contract::{
    AuthorityReleaseId, CalendarId, EpochId, EventId, TimeAcquisitionId, TimeSourceId,
};

pub(crate) struct TimeDb {
    /// Two replicas' pools, each connected as its own registration login.
    pub(crate) a: PlatformStore,
    pub(crate) b: PlatformStore,
    /// Owns the database; dropping it drops the database.
    _fixture: TestDb,
}

impl TimeDb {
    pub(crate) async fn new() -> Self {
        let fixture = TestDb::new().await;
        let slug = slug();
        fixture
            .migrate_server(&slug, crate::persistence::migrator())
            .await;
        let a = fixture.server_store(&slug).await;
        let b = fixture.server_store(&slug).await;
        Self {
            a,
            b,
            _fixture: fixture,
        }
    }
}

pub(crate) fn slug() -> ServerSlug {
    crate::changes::server()
}

/// A stored row, named by its table and key, for the corruption queries.
#[derive(Clone, Copy)]
pub(crate) enum Row<'a> {
    Source(&'a TimeSourceId),
    Release(&'a AuthorityReleaseId),
    Acquisition(&'a TimeAcquisitionId),
    Calendar(&'a CalendarId),
    Epoch(&'a EpochId),
    Event(&'a EventId),
    ClockPolicy,
}

impl Row<'_> {
    fn table(self) -> &'static str {
        match self {
            Self::Source(_) => "source",
            Self::Release(_) => "authority_release",
            Self::Acquisition(_) => "acquisition",
            Self::Calendar(_) => "calendar_version",
            Self::Epoch(_) => "epoch",
            Self::Event(_) => "event",
            Self::ClockPolicy => "clock_policy",
        }
    }

    /// The row's key; the tenant alone names its clock policy.
    fn key_text(&self) -> &str {
        match self {
            Self::Source(key) => key.as_str(),
            Self::Release(key) => key.as_str(),
            Self::Acquisition(key) => key.as_str(),
            Self::Calendar(key) => key.as_str(),
            Self::Epoch(key) => key.as_str(),
            Self::Event(key) => key.as_str(),
            Self::ClockPolicy => "",
        }
    }
}

/// The stored body of `row`.
pub(crate) async fn body(
    store: &PlatformStore,
    tenant: TenantId,
    row: Row<'_>,
) -> serde_json::Value {
    sqlx::query_file_scalar!(
        "src/tests/queries/body.sql",
        row.table(),
        tenant as TenantId,
        row.key_text(),
    )
    .fetch_one(store.pool())
    .await
    .expect("fixture row has a body")
}

/// Replace the stored body of `row`, bypassing catalog admission.
pub(crate) async fn set_body(
    store: &PlatformStore,
    tenant: TenantId,
    row: Row<'_>,
    body: &serde_json::Value,
) {
    let updated = sqlx::query_file_scalar!(
        "src/tests/queries/set_body.sql",
        row.table(),
        tenant as TenantId,
        row.key_text(),
        body,
    )
    .fetch_one(store.pool())
    .await
    .expect("fixture body update");
    assert_eq!(updated, 1, "fixture body update names one row");
}

/// Set the stored record version of `row`; the schema's checks still apply.
pub(crate) async fn set_record_version(
    store: &PlatformStore,
    tenant: TenantId,
    row: Row<'_>,
    version: i64,
) -> Result<(), sqlx::Error> {
    let updated = sqlx::query_file_scalar!(
        "src/tests/queries/set_record_version.sql",
        row.table(),
        tenant as TenantId,
        row.key_text(),
        version,
    )
    .fetch_one(store.pool())
    .await?;
    assert_eq!(updated, 1, "fixture version update names one row");
    Ok(())
}

/// Whether `error` is a violated schema check, foreign key, uniqueness or range.
pub(crate) fn rejected_by_schema(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|database| {
        database.is_check_violation()
            || database.is_foreign_key_violation()
            || database.is_unique_violation()
            || database.code().as_deref() == Some("22003")
    })
}
