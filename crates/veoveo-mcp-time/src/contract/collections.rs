//! Collection identities, SQL total orders and contextual continuation policies.
use super::*;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, fmt};
use veoveo_hub_sdk::definition::NoFilters;
use veoveo_types::{
    Collection, CursorError, CursorScope, LiveKeyset, PageSize, PagingPolicy, PrincipalId,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarPosition {
    pub calendar_id: CalendarId,
    pub version: TimeVersion,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EpochPosition {
    pub epoch_id: EpochId,
    pub version: TimeVersion,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventPosition {
    pub tai_seconds: i64,
    pub nanosecond: SubsecondNanoseconds,
    pub event_id: EventId,
}

pub struct Sources;
impl Collection for Sources {
    type Item = TimeSource;
    type Filters = NoFilters;
    type Key = TimeSourceId;
    type Limit = PageSize<100>;
    type Policy = LiveKeyset;
    const NAME: &'static str = "time.sources";
    fn key(item: &TimeSource) -> TimeSourceId {
        item.source_id.clone()
    }
}
pub struct Acquisitions;
impl Collection for Acquisitions {
    type Item = TimeAcquisition;
    type Filters = NoFilters;
    type Key = TimeAcquisitionId;
    type Limit = PageSize<100>;
    type Policy = LiveKeyset;
    const NAME: &'static str = "time.acquisitions";
    fn key(item: &TimeAcquisition) -> TimeAcquisitionId {
        item.acquisition_id.clone()
    }
    fn compare(_: &NoFilters, a: &TimeAcquisitionId, b: &TimeAcquisitionId) -> Ordering {
        b.cmp(a)
    }
}
pub struct ActiveAuthorities;
impl Collection for ActiveAuthorities {
    type Item = ActiveAuthoritySelection;
    type Filters = NoFilters;
    type Key = String;
    type Limit = PageSize<100>;
    type Policy = LiveKeyset;
    const NAME: &'static str = "time.active_authorities";
    fn key(item: &ActiveAuthoritySelection) -> String {
        match item.release.dataset_kind {
            AuthorityDatasetKind::LeapSeconds => "leap_seconds",
            AuthorityDatasetKind::Tzdb => "tzdb",
        }
        .to_owned()
    }
}
pub struct Calendars;
impl Collection for Calendars {
    type Item = OperationalCalendar;
    type Filters = NoFilters;
    type Key = CalendarPosition;
    type Limit = PageSize<100>;
    type Policy = LiveKeyset;
    const NAME: &'static str = "time.calendars";
    fn key(item: &OperationalCalendar) -> CalendarPosition {
        CalendarPosition {
            calendar_id: item.calendar_id.clone(),
            version: item.version,
        }
    }
    fn compare(_: &NoFilters, a: &CalendarPosition, b: &CalendarPosition) -> Ordering {
        a.calendar_id
            .cmp(&b.calendar_id)
            .then_with(|| b.version.cmp(&a.version))
    }
}
pub struct Epochs;
impl Collection for Epochs {
    type Item = Epoch;
    type Filters = NoFilters;
    type Key = EpochPosition;
    type Limit = PageSize<100>;
    type Policy = LiveKeyset;
    const NAME: &'static str = "time.epochs";
    fn key(item: &Epoch) -> EpochPosition {
        EpochPosition {
            epoch_id: item.epoch_id.clone(),
            version: item.version,
        }
    }
    fn compare(_: &NoFilters, a: &EpochPosition, b: &EpochPosition) -> Ordering {
        a.epoch_id
            .cmp(&b.epoch_id)
            .then_with(|| b.version.cmp(&a.version))
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<EventState>,
}
pub struct Events;
impl Collection for Events {
    type Item = Event;
    type Filters = EventFilters;
    type Key = EventPosition;
    type Limit = PageSize<100>;
    type Policy = EventKeyset;
    const NAME: &'static str = "time.events";
    fn key(item: &Event) -> EventPosition {
        EventPosition {
            tai_seconds: item.due.tai_seconds_since_1970,
            nanosecond: item.due.nanosecond,
            event_id: item.event_id.clone(),
        }
    }
}
pub struct EventKeyset {
    principal: PrincipalId,
}
impl EventKeyset {
    pub fn new(principal: PrincipalId) -> Self {
        Self { principal }
    }
}
impl PagingPolicy<Events> for EventKeyset {
    fn scope(
        &self,
        tenant: impl fmt::Display,
        filters: &EventFilters,
        limit: u32,
    ) -> Result<CursorScope, CursorError> {
        CursorScope::new(Events::NAME, tenant, &(filters, limit, &self.principal))
    }
}
/// Public release metadata; local compiled artifact paths remain server-owned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseEntry {
    pub release_uri: TimeAuthorityReleaseUri,
    pub dataset_kind: AuthorityDatasetKind,
    pub version_label: String,
    pub source_digest: veoveo_types::Sha256Digest,
    pub state: AuthorityReleaseState,
}
impl From<AuthorityRelease> for ReleaseEntry {
    fn from(release: AuthorityRelease) -> Self {
        Self {
            release_uri: TimeAuthorityReleaseUri::new(&release.release_id),
            dataset_kind: release.dataset_kind,
            version_label: release.version_label.clone(),
            source_digest: release.source_digest.clone(),
            state: release.state,
        }
    }
}
pub struct Releases;
impl Collection for Releases {
    type Item = ReleaseEntry;
    type Filters = NoFilters;
    type Key = AuthorityReleaseId;
    type Limit = PageSize<100>;
    type Policy = LiveKeyset;
    const NAME: &'static str = "time.releases";
    fn key(item: &ReleaseEntry) -> AuthorityReleaseId {
        item.release_uri.release_id().clone()
    }
}
pub struct BootstrapAuthorities;
impl Collection for BootstrapAuthorities {
    type Item = TimeAuthorityReference;
    type Filters = NoFilters;
    type Key = AuthorityReleaseId;
    type Limit = PageSize<100>;
    type Policy = LiveKeyset;
    const NAME: &'static str = "time.bootstrap_authorities";
    fn key(item: &TimeAuthorityReference) -> AuthorityReleaseId {
        item.release_id().clone()
    }
}
