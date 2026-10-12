use super::*;

#[derive(Clone, Debug)]
pub(crate) struct TimeSourceDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) source_key: TimeSourceId,
    pub(crate) name: String,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) source_url: String,
    pub(crate) expected_content_type: String,
    pub(crate) enabled: bool,
    pub(crate) body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeAuthorityReleaseDraft {
    pub(crate) work_context: veoveo_types::WorkContextId,
    pub(crate) identity: PlatformIdentity,
    pub(crate) release_key: AuthorityReleaseId,
    pub(crate) source_key: TimeSourceId,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) state: TimeAuthorityReleaseState,
    pub(crate) version_label: String,
    pub(crate) source_url: String,
    pub(crate) source_digest: Sha256Digest,
    pub(crate) artifact_path: String,
    pub(crate) retrieved_at: DateTime<Utc>,
    pub(crate) validated_at: DateTime<Utc>,
    pub(crate) body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeAcquisitionDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) acquisition_key: TimeAcquisitionId,
    pub(crate) source_key: TimeSourceId,
    pub(crate) expected_source_digest: Option<Sha256Digest>,
    pub(crate) idempotency_key: String,
    pub(crate) status: TimeAcquisitionState,
    pub(crate) phase: TimeAcquisitionPhase,
    pub(crate) staged_release_key: Option<AuthorityReleaseId>,
    pub(crate) body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeAcquisitionUpdate {
    pub(crate) tenant_id: TenantId,
    pub(crate) acquisition_key: TimeAcquisitionId,
    pub(crate) expected_record_version: TimeVersion,
    pub(crate) status: TimeAcquisitionState,
    pub(crate) phase: TimeAcquisitionPhase,
    pub(crate) staged_release_key: Option<AuthorityReleaseId>,
    pub(crate) body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeCalendarVersionDraft {
    pub(crate) work_context: veoveo_types::WorkContextId,
    pub(crate) identity: PlatformIdentity,
    pub(crate) calendar_key: CalendarId,
    pub(crate) calendar_version: TimeVersion,
    pub(crate) name: String,
    pub(crate) zone_id: String,
    pub(crate) state: TimeCalendarState,
    pub(crate) body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeEpochDraft {
    pub(crate) work_context: veoveo_types::WorkContextId,
    pub(crate) identity: PlatformIdentity,
    pub(crate) epoch_key: EpochId,
    pub(crate) name: String,
    pub(crate) epoch_version: TimeVersion,
    pub(crate) tai_seconds_since_1970: i64,
    pub(crate) nanosecond: SubsecondNanoseconds,
    pub(crate) body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeEventDraft {
    pub(crate) work_context: veoveo_types::WorkContextId,
    pub(crate) identity: PlatformIdentity,
    pub(crate) event_key: EventId,
    pub(crate) name: String,
    pub(crate) state: TimeEventState,
    pub(crate) due_tai_seconds_since_1970: i64,
    pub(crate) due_nanosecond: SubsecondNanoseconds,
    /// The UTC instant at which the event becomes due.
    pub(crate) due: DateTime<Utc>,
    pub(crate) idempotency_key: String,
    pub(crate) body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeClockPolicyDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) policy: ClockQualityPolicy,
}
