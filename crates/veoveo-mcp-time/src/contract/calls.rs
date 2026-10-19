//! Runtime-free tool cardinality and typed resource reads.
use super::*;
use veoveo_hub_sdk::{
    calls::{ProviderContract, TaskToolCall, ToolCall},
    definition::{CollectionResource, NoFilters, Product},
};
use veoveo_types::ServerSlug;

pub struct TimeProvider;
impl ProviderContract for TimeProvider {
    fn slug() -> ServerSlug {
        ServerSlug::parse("time").expect("Time slug")
    }
}

#[veoveo_hub_sdk::collection_tool(provider = TimeProvider, collection = Sources, name = "list_sources")]
pub struct ListSources;
#[veoveo_hub_sdk::collection_tool(provider = TimeProvider, collection = Acquisitions, name = "list_acquisitions")]
pub struct ListAcquisitions;
#[veoveo_hub_sdk::collection_tool(provider = TimeProvider, collection = ActiveAuthorities, name = "list_active_authorities")]
pub struct ListActiveAuthorities;

impl ToolCall for ResolveTimeRequest {
    type Provider = TimeProvider;
    type Output = ResolveTimeOutput;
    const TOOL: &'static str = "resolve_time";
}

impl ToolCall for ConvertTimeRequest {
    type Provider = TimeProvider;
    type Output = ConvertTimeOutput;
    const TOOL: &'static str = "convert_time";
}

impl ToolCall for EvaluateWindowsRequest {
    type Provider = TimeProvider;
    type Output = EvaluateWindowsOutput;
    const TOOL: &'static str = "evaluate_windows";
}

impl ToolCall for AssessClockRequest {
    type Provider = TimeProvider;
    type Output = ClockAssessment;
    const TOOL: &'static str = "assess_clock";
}

impl ToolCall for ExpandScheduleRequest {
    type Provider = TimeProvider;
    type Output = ExpandScheduleOutput;
    const TOOL: &'static str = "expand_schedule";
}

impl ToolCall for ValidateTimelineRequest {
    type Provider = TimeProvider;
    type Output = ValidateTimelineOutput;
    const TOOL: &'static str = "validate_timeline";
}

impl ToolCall for CreateEventRequest {
    type Provider = TimeProvider;
    type Output = Product<Event>;
    const TOOL: &'static str = "create_event";
}

impl ToolCall for CancelEventRequest {
    type Provider = TimeProvider;
    type Output = Event;
    const TOOL: &'static str = "cancel_event";
}

impl ToolCall for GetSourceRequest {
    type Provider = TimeProvider;
    type Output = TimeSource;
    const TOOL: &'static str = "get_source";
}

impl ToolCall for CreateSourceRequest {
    type Provider = TimeProvider;
    type Output = TimeSource;
    const TOOL: &'static str = "create_source";
}

impl ToolCall for ReplaceSourceRequest {
    type Provider = TimeProvider;
    type Output = TimeSource;
    const TOOL: &'static str = "replace_source";
}

impl ToolCall for GetAcquisitionRequest {
    type Provider = TimeProvider;
    type Output = TimeAcquisition;
    const TOOL: &'static str = "get_acquisition";
}

impl ToolCall for StartAcquisitionRequest {
    type Provider = TimeProvider;
    type Output = TimeAcquisition;
    const TOOL: &'static str = "start_acquisition";
}

impl ToolCall for CancelAcquisitionRequest {
    type Provider = TimeProvider;
    type Output = TimeAcquisition;
    const TOOL: &'static str = "cancel_acquisition";
}

impl ToolCall for ActivateReleaseRequest {
    type Provider = TimeProvider;
    type Output = AuthorityRelease;
    const TOOL: &'static str = "activate_release";
}

impl ToolCall for CreateCalendarRequest {
    type Provider = TimeProvider;
    type Output = OperationalCalendar;
    const TOOL: &'static str = "create_calendar";
}

impl ToolCall for UpsertEpochRequest {
    type Provider = TimeProvider;
    type Output = Epoch;
    const TOOL: &'static str = "create_epoch";
}

impl ToolCall for ReplaceClockQualityPolicyRequest {
    type Provider = TimeProvider;
    type Output = ClockQualityPolicyRecord;
    const TOOL: &'static str = "replace_clock_policy";
}

impl TaskToolCall for ExpandScheduleRequest {}
impl TaskToolCall for ValidateTimelineRequest {}
impl veoveo_hub_sdk::definition::SingleOutput for ResolveTimeOutput {}
impl veoveo_hub_sdk::definition::SingleOutput for ConvertTimeOutput {}
impl veoveo_hub_sdk::definition::SingleOutput for EvaluateWindowsOutput {}
impl veoveo_hub_sdk::definition::SingleOutput for ClockAssessment {}
impl veoveo_hub_sdk::definition::SingleOutput for ExpandScheduleOutput {}
impl veoveo_hub_sdk::definition::SingleOutput for ValidateTimelineOutput {}
impl veoveo_hub_sdk::definition::SingleOutput for Event {}
impl veoveo_hub_sdk::definition::SingleOutput for TimeSource {}
impl veoveo_hub_sdk::definition::SingleOutput for TimeAcquisition {}
impl veoveo_hub_sdk::definition::SingleOutput for AuthorityRelease {}
impl veoveo_hub_sdk::definition::SingleOutput for OperationalCalendar {}
impl veoveo_hub_sdk::definition::SingleOutput for Epoch {}
impl veoveo_hub_sdk::definition::SingleOutput for ClockQualityPolicyRecord {}
impl veoveo_hub_sdk::definition::SingleOutput for ClockQuality {}
impl veoveo_hub_sdk::definition::SingleOutput for ClockCurrent {}
impl veoveo_hub_sdk::definition::SingleOutput for EffectiveTimeAuthority {}
impl veoveo_hub_sdk::definition::SingleOutput for TimeAuthorityReference {}

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::collection_resource(provider = TimeProvider, address_type = TimeResource,
    collection = Calendars, template = "time://calendars", address = TimeResource::Calendars,
    filters = NoFilters {}, parse = |address| matches!(address, TimeResource::Calendars).then_some(Self))]
pub struct ReadCalendars;

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::collection_resource(provider = TimeProvider, address_type = TimeResource,
    collection = Epochs, template = "time://epochs", address = TimeResource::Epochs,
    filters = NoFilters {}, parse = |address| matches!(address, TimeResource::Epochs).then_some(Self))]
pub struct ReadEpochs;

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::collection_resource(provider = TimeProvider, address_type = TimeResource,
    collection = Releases, template = "time://authorities/releases", address = TimeResource::AuthorityReleases,
    filters = NoFilters {}, parse = |address| matches!(address, TimeResource::AuthorityReleases).then_some(Self))]
pub struct ReadReleases;

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::collection_resource(provider = TimeProvider, address_type = TimeResource,
    collection = BootstrapAuthorities, template = "time://authorities/bootstrap", address = TimeResource::BootstrapAuthorities,
    filters = NoFilters {}, parse = |address| matches!(address, TimeResource::BootstrapAuthorities).then_some(Self))]
pub struct ReadBootstrapAuthorities;

#[derive(Clone, Debug)]
pub struct ReadEvents;
impl CollectionResource for ReadEvents {
    type Provider = TimeProvider;
    type Address = TimeResource;
    type Collection = Events;
    const TEMPLATE: &'static str = "time://events";
    fn address(&self) -> TimeResource {
        TimeResource::Events
    }
    fn filters(&self) -> EventFilters {
        EventFilters::default()
    }
    fn from_address(address: &TimeResource) -> Option<Self> {
        matches!(address, TimeResource::Events).then_some(Self)
    }
}

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = ClockCurrent,
    address = TimeResource::ClockCurrent)]
pub struct ReadClockCurrent;

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = ClockQuality,
    address = TimeResource::ClockQuality)]
pub struct ReadClockQuality;

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = EffectiveTimeAuthority,
    address = TimeResource::AuthoritiesCurrent)]
pub struct ReadAuthoritiesCurrent;

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = OperationalCalendar,
    template = "time://calendars/{calendar_id}/versions/{version}", address = TimeResource::Calendar {id: self.id.clone(), version: self.version},
    parse = |address| match address { TimeResource::Calendar {id, version} => Some(Self {id:id.clone(),version:*version}), _=>None })]
pub struct ReadCalendar {
    pub id: CalendarId,
    pub version: TimeVersion,
}

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = Epoch,
    template = "time://epochs/{epoch_id}/versions/{version}", address = TimeResource::EpochVersion {id: self.id.clone(), version: self.version},
    parse = |address| match address { TimeResource::EpochVersion {id, version} => Some(Self {id:id.clone(),version:*version}), _=>None })]
pub struct ReadEpochVersion {
    pub id: EpochId,
    pub version: TimeVersion,
}

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = Epoch,
    template = "time://epochs/{epoch_id}", address = TimeResource::Epoch(self.id.clone()),
    parse = |address| match address { TimeResource::Epoch(id) => Some(Self {id:id.clone()}), _=>None })]
pub struct ReadEpoch {
    pub id: EpochId,
}

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = Event,
    template = "time://events/{event_id}", address = TimeResource::Event(self.id.clone()),
    parse = |address| match address { TimeResource::Event(id) => Some(Self {id:id.clone()}), _=>None })]
pub struct ReadEvent {
    pub id: EventId,
}

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = TimeAuthorityReference,
    template = "time://authorities/releases/{release_id}", address = TimeResource::AuthorityRelease(self.id.clone()),
    parse = |address| match address { TimeResource::AuthorityRelease(id) => Some(Self {id:id.clone()}), _=>None })]
pub struct ReadAuthorityRelease {
    pub id: AuthorityReleaseId,
}

#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = TimeAuthorityReference,
    template = "time://authorities/bootstrap/{release_id}", address = TimeResource::BootstrapAuthority(self.id.clone()),
    parse = |address| match address { TimeResource::BootstrapAuthority(id) => Some(Self {id:id.clone()}), _=>None })]
pub struct ReadBootstrapAuthority {
    pub id: AuthorityReleaseId,
}

#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    veoveo_hub_sdk::SingleOutput,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZoneView {
    pub zone_id: String,
    pub tzdb_release_id: AuthorityReleaseId,
    pub current: Option<ZonedRepresentation>,
}
#[derive(Clone, Debug)]
#[veoveo_hub_sdk::resource_read(provider = TimeProvider, address_type = TimeResource, content = ZoneView,
    template = "time://zones/{+zone_id}", address = TimeResource::Zone(self.zone.clone()),
    parse = |address| match address { TimeResource::Zone(zone) => Some(Self {zone:zone.clone()}), _=>None })]
pub struct ReadZone {
    pub zone: TimeZoneId,
}
