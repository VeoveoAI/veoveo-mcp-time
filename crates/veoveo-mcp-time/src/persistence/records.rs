//! Driver records. Typed columns decode into their domain types; the catalog checks that
//! each JSON body agrees with its columns.

use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, veoveo_types::Vocabulary)]
#[vocabulary(postgres)]
pub(crate) enum TimeDatasetKind {
    #[vocabulary(rename = "tzdb")]
    Tzdb,
    #[vocabulary(rename = "leap_seconds")]
    LeapSeconds,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(postgres)]
pub(crate) enum TimeAuthorityReleaseState {
    #[vocabulary(rename = "staged")]
    Staged,
    #[vocabulary(rename = "active")]
    Active,
    #[vocabulary(rename = "retired")]
    Retired,
    #[vocabulary(rename = "quarantined")]
    Quarantined,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(postgres)]
pub(crate) enum TimeAcquisitionState {
    #[vocabulary(rename = "queued")]
    Queued,
    #[vocabulary(rename = "running")]
    Running,
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancel_requested")]
    CancelRequested,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(postgres)]
pub(crate) enum TimeCalendarState {
    #[vocabulary(rename = "staged")]
    Staged,
    #[vocabulary(rename = "active")]
    Active,
    #[vocabulary(rename = "retired")]
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(postgres)]
pub(crate) enum TimeEventState {
    #[vocabulary(rename = "scheduled")]
    Scheduled,
    #[vocabulary(rename = "due")]
    Due,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeSourceRecord {
    pub(crate) tenant: TenantId,
    pub(crate) source_key: TimeSourceId,
    pub(crate) owner: PrincipalId,
    pub(crate) name: String,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) source_url: String,
    pub(crate) expected_content_type: String,
    pub(crate) enabled: bool,
    pub(crate) body: serde_json::Value,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeAuthorityReleaseRecord {
    pub(crate) tenant: TenantId,
    pub(crate) release_key: AuthorityReleaseId,
    pub(crate) owner: PrincipalId,
    pub(crate) source_key: TimeSourceId,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) state: TimeAuthorityReleaseState,
    pub(crate) version_label: String,
    pub(crate) source_url: String,
    pub(crate) source_digest: String,
    pub(crate) artifact_path: String,
    pub(crate) retrieved_at: DateTime<Utc>,
    pub(crate) validated_at: DateTime<Utc>,
    pub(crate) body: serde_json::Value,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) tenant_key: String,
    pub(crate) owner_key: String,
    pub(crate) work_context: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeActiveAuthorityRecord {
    pub(crate) tenant: TenantId,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) release_key: AuthorityReleaseId,
    pub(crate) previous_release_key: Option<AuthorityReleaseId>,
    pub(crate) activated_by: PrincipalId,
    pub(crate) activated_at: DateTime<Utc>,
    pub(crate) record_version: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeAcquisitionRecord {
    pub(crate) tenant: TenantId,
    pub(crate) acquisition_key: TimeAcquisitionId,
    pub(crate) owner: PrincipalId,
    pub(crate) source_key: TimeSourceId,
    pub(crate) expected_source_digest: Option<String>,
    pub(crate) idempotency_key: String,
    pub(crate) status: TimeAcquisitionState,
    pub(crate) phase: TimeAcquisitionPhase,
    pub(crate) staged_release_key: Option<AuthorityReleaseId>,
    pub(crate) body: serde_json::Value,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeCalendarVersionRecord {
    pub(crate) tenant: TenantId,
    pub(crate) calendar_key: CalendarId,
    pub(crate) calendar_version: i64,
    pub(crate) owner: PrincipalId,
    pub(crate) name: String,
    pub(crate) zone_id: String,
    pub(crate) state: TimeCalendarState,
    pub(crate) body: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) tenant_key: String,
    pub(crate) owner_key: String,
    pub(crate) work_context: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeEpochRecord {
    pub(crate) tenant: TenantId,
    pub(crate) epoch_key: EpochId,
    pub(crate) epoch_version: i64,
    pub(crate) owner: PrincipalId,
    pub(crate) name: String,
    pub(crate) tai_seconds_since_1970: i64,
    pub(crate) nanosecond: i32,
    pub(crate) body: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) tenant_key: String,
    pub(crate) owner_key: String,
    pub(crate) work_context: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeEventRecord {
    pub(crate) tenant: TenantId,
    pub(crate) event_key: EventId,
    pub(crate) owner: PrincipalId,
    pub(crate) name: String,
    pub(crate) state: TimeEventState,
    pub(crate) due_tai_seconds_since_1970: i64,
    pub(crate) due_nanosecond: i32,
    pub(crate) due: DateTime<Utc>,
    pub(crate) idempotency_key: String,
    pub(crate) body: serde_json::Value,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) tenant_key: String,
    pub(crate) owner_key: String,
    pub(crate) work_context: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimeClockPolicyRecord {
    pub(crate) tenant: TenantId,
    pub(crate) owner: PrincipalId,
    pub(crate) maximum_error_nanoseconds: i64,
    pub(crate) maximum_stratum: i16,
    pub(crate) minimum_source_diversity: i64,
    pub(crate) maximum_holdover_seconds: i64,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}
