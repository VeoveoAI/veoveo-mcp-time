use std::sync::{Arc, LazyLock};

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ClientCapabilities, CompleteRequestParams, CompleteResult, CompletionInfo, Reference},
    service::RequestContext,
};
use uuid::Uuid;
use veoveo_hub_sdk::{
    definition::Product,
    docs::ServerDocs,
    hosting::{DomainServer, RoutePolicy, Routes, completion, rank_completions},
    server_contract::McpServerSetup,
};

use crate::{
    clock::assess_clock,
    contract::{
        AssessClockRequest, CancelEventRequest, ClockAssessment, ClockQualityPolicy,
        ConvertTimeOutput, ConvertTimeRequest, CreateEventRequest, EvaluateWindowsOutput,
        EvaluateWindowsRequest, Event, EventId, EventState, ExpandScheduleOutput,
        ExpandScheduleRequest, ResolveTimeOutput, ResolveTimeRequest, ValidateTimelineOutput,
        ValidateTimelineRequest,
    },
    prompts::TimePrompt,
    state::TimeApplication,
    uris,
};

mod administration;
pub(crate) mod declarations;
pub(crate) mod requirements;
mod resources;
mod setup;
mod subscriptions;
use requirements::require_permission;
pub(crate) use setup::SERVER_SETUP;
use setup::TimeContract;
pub(crate) use subscriptions::TimeSubscriptions;

/// The crate documents embedded at build time and served under the well-known
/// surface: `time://docs`, `time://docs/{doc_id}`, `time://contract`, and the
/// host's `admin/docs` routes (contract C18-C21).
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_hub_sdk::server_docs!("time"));

#[derive(Clone)]
pub struct TimeMcp {
    state: Arc<TimeApplication>,
}

impl TimeMcp {
    pub fn new(state: Arc<TimeApplication>) -> Self {
        Self { state }
    }

    /// The capability inventory declared at `time://contract` (contract C19).
    ///
    async fn resolve_time(
        &self,
        request: ResolveTimeRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ResolveTimeOutput, McpError> {
        let identity = veoveo_hub_sdk::hosting::hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine_for_expressions(&scope, [&request.expression])
            .await
            .map_err(internal)?
            .resolve(&request)
            .map_err(invalid_params)?;
        Ok(output)
    }

    async fn convert_time(
        &self,
        request: ConvertTimeRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ConvertTimeOutput, McpError> {
        let identity = veoveo_hub_sdk::hosting::hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine(&scope)
            .await
            .map_err(internal)?
            .convert(&request)
            .map_err(invalid_params)?;
        Ok(output)
    }

    async fn evaluate_windows(
        &self,
        request: EvaluateWindowsRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<EvaluateWindowsOutput, McpError> {
        let identity = veoveo_hub_sdk::hosting::hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine(&scope)
            .await
            .map_err(internal)?
            .evaluate_windows(&request)
            .map_err(invalid_params)?;
        Ok(output)
    }

    async fn assess_clock(
        &self,
        request: AssessClockRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ClockAssessment, McpError> {
        let identity = veoveo_hub_sdk::hosting::hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let policy = match request.policy {
            Some(policy) => policy,
            None => self
                .state
                .catalog
                .clock_policy(&scope)
                .await
                .map_err(internal)?
                .map(|value| value.0)
                .unwrap_or_else(default_clock_policy),
        };
        let output = assess_clock(self.state.clock.quality().await.map_err(internal)?, policy)
            .map_err(internal)?;
        Ok(output)
    }

    async fn expand_schedule(
        &self,
        _request: ExpandScheduleRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<ExpandScheduleOutput, McpError> {
        Err(McpError::missing_required_client_capability(
            ClientCapabilities::builder().enable_tasks().build(),
        ))
    }

    async fn validate_timeline(
        &self,
        _request: ValidateTimelineRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<ValidateTimelineOutput, McpError> {
        Err(McpError::missing_required_client_capability(
            ClientCapabilities::builder().enable_tasks().build(),
        ))
    }

    async fn create_event(
        &self,
        request: CreateEventRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<Product<Event>, McpError> {
        let identity = veoveo_hub_sdk::hosting::hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let engine = self.state.engine(&scope).await.map_err(internal)?;
        engine
            .convert(&ConvertTimeRequest {
                instant: request.due.clone(),
                zone_ids: Vec::new(),
                scales: Vec::new(),
            })
            .map_err(invalid_params)?;
        let due = engine.utc_due(&request.due).map_err(invalid_params)?;
        if request.name.trim().is_empty()
            || request.name.len() > 256
            || request.idempotency_key.trim().is_empty()
        {
            return Err(invalid_params(
                "event name and idempotency key must be non-empty",
            ));
        }
        let event = Event {
            event_id: EventId::parse(format!("event-{}", Uuid::now_v7()))
                .map_err(invalid_params)?,
            name: request.name,
            due: request.due,
            state: EventState::Scheduled,
            record_version: crate::TimeVersion::new(1).unwrap(),
        };
        let event = self
            .state
            .catalog
            .create_event(&scope, event, due, request.idempotency_key)
            .await
            .map_err(internal)?;
        Product::new(&crate::TimeResource::Event(event.event_id.clone()), event).map_err(internal)
    }

    async fn cancel_event(
        &self,
        request: CancelEventRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<Event, McpError> {
        let identity = veoveo_hub_sdk::hosting::hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let event = self
            .state
            .catalog
            .cancel_event(&scope, &request.event_id, request.expected_record_version)
            .await
            .map_err(internal)?;
        Ok(event)
    }
}

static ROUTES: LazyLock<Routes<TimeMcp>> = LazyLock::new(|| {
    use crate::contract::*;
    use TimePermission::*;
    let policy = |permission| RoutePolicy::permission(permission).expect("Time permission");
    let routes = Routes::<TimeMcp>::builder(policy(AuthoritiesRead));
    let routes = routes.tool::<ResolveTimeRequest, _, _>(
        "Resolve a time given as RFC 3339/9557, civil time, a military DTG, Unix time, TAI, GPS time, Julian TAI, or mission-relative time, using the active time-authority releases.",
        policy(AuthoritiesRead),
        |domain, request, context| async move { domain.resolve_time(request, context).await.map_err(Into::into) },
    );
    let routes = routes.tool::<ConvertTimeRequest, _, _>(
        "Convert a resolved TimeInstant to UTC, chosen IANA time zones, TAI, TT, TDB, GPS time (GPST), and Galileo time (GST).",
        policy(AuthoritiesRead),
        |domain, request, context| async move { domain.convert_time(request, context).await.map_err(Into::into) },
    );
    let routes = routes.tool::<EvaluateWindowsRequest, _, _>(
        "Compute the union, intersection, or difference of half-open time windows.",
        policy(SchedulesExpand),
        |domain, request, context| async move {
            domain
                .evaluate_windows(request, context)
                .await
                .map_err(Into::into)
        },
    );
    let routes = routes.tool::<AssessClockRequest, _, _>(
        "Check the host clock's measured offset, error bound, stratum, source diversity, holdover, and traceability against a policy you pass or the tenant's policy.",
        policy(AuthoritiesRead),
        |domain, request, context| async move { domain.assess_clock(request, context).await.map_err(Into::into) },
    );
    let routes = routes.task::<ExpandScheduleRequest, _, _>(
        "Expand a versioned civil-time operational calendar into half-open time windows. Run as an MCP Task.",
        policy(SchedulesExpand),
        |domain, request, context| async move { domain.expand_schedule(request, context).await.map_err(Into::into) },
    );
    let routes = routes.task::<ValidateTimelineRequest, _, _>(
        "Resolve named time points and check their ordering and minimum and maximum separation constraints. Run as an MCP Task.",
        policy(TimelinesValidate),
        |domain, request, context| async move { domain.validate_timeline(request, context).await.map_err(Into::into) },
    );
    let routes = routes.tool::<CreateEventRequest, _, _>(
        "Create an event at a resolved time instant. Subscribers receive a resource update when it becomes due. Only its owner can change it.",
        policy(EventsWrite),
        |domain, request, context| async move { domain.create_event(request, context).await.map_err(Into::into) },
    );
    let routes = routes.tool::<CancelEventRequest, _, _>(
        "Cancel an event you own. Pass the revision you last read; the call fails if it has changed.",
        policy(EventsWrite),
        |domain, request, context| async move { domain.cancel_event(request, context).await.map_err(Into::into) },
    );
    let routes = routes.collection::<ListSources, _, _>(
        "List the tenant's registered time-authority sources, the HTTPS locations of IANA time zone and leap-second data, in source ID order. A page holds up to `limit` sources, 100 by default. Pass its `nextCursor` as `cursor` to read the next page.",
        policy(AuthoritiesInspect),
        |domain, request, context| async move { domain.list_sources(request, context).await },
    );
    let routes = routes.tool::<GetSourceRequest, _, _>(
        "Read one registered time-authority source.",
        policy(AuthoritiesInspect),
        |domain, request, context| async move { domain.get_source(request, context).await },
    );
    let routes = routes.tool::<CreateSourceRequest, _, _>(
        "Register an HTTPS source of IANA time zone or leap-second data for the tenant. Repeating a call with the same idempotency key returns the same source.",
        policy(AuthoritiesAdminister),
        |domain, request, context| async move { domain.create_source(request, context).await },
    );
    let routes = routes.tool::<ReplaceSourceRequest, _, _>(
        "Replace a registered source's settings. Pass the record version you last read; the call fails if it has changed.",
        policy(AuthoritiesAdminister),
        |domain, request, context| async move { domain.replace_source(request, context).await },
    );
    let routes = routes.collection::<ListAcquisitions, _, _>(
        "List the tenant's authority acquisitions and their progress, the most recently started first. A page holds up to `limit` acquisitions, 100 by default. Pass its `nextCursor` as `cursor` to read the next page.",
        policy(AuthoritiesInspect),
        |domain, request, context| async move { domain.list_acquisitions(request, context).await },
    );
    let routes = routes.tool::<GetAcquisitionRequest, _, _>(
        "Read one acquisition's status, phase and staged release.",
        policy(AuthoritiesInspect),
        |domain, request, context| async move { domain.get_acquisition(request, context).await },
    );
    let routes = routes.tool::<StartAcquisitionRequest, _, _>(
        "Download, verify and compile a new release from a registered source, staging it for activation. The acquisition continues after the call returns; read its progress with get_acquisition. Repeating a call with the same idempotency key returns the same acquisition.",
        policy(AuthoritiesAdminister),
        |domain, request, context| async move { domain.start_acquisition(request, context).await },
    );
    let routes = routes.tool::<CancelAcquisitionRequest, _, _>(
        "Request cancellation of a running acquisition. Its scratch content is removed and no release is staged.",
        policy(AuthoritiesAdminister),
        |domain, request, context| async move { domain.cancel_acquisition(request, context).await },
    );
    let routes = routes.collection::<ListActiveAuthorities, _, _>(
        "List the tenant's active release for each authority dataset with the pointer version activate_release requires as its expected active pointer version.",
        policy(AuthoritiesInspect),
        |domain, request, context| async move { domain.list_active_authorities(request, context).await },
    );
    let routes = routes.tool::<ActivateReleaseRequest, _, _>(
        "Make a staged release the tenant's active authority for its dataset. The previous release is retired and kept. Pass the release's record version and the active pointer version you last read; the call fails if either has changed.",
        policy(AuthoritiesAdminister),
        |domain, request, context| async move { domain.activate_release(request, context).await },
    );
    let routes = routes.tool::<CreateCalendarRequest, _, _>(
        "Publish a version of an operational calendar for the tenant. Repeating a call with the same idempotency key returns the same version.",
        policy(CalendarsWrite),
        |domain, request, context| async move { domain.create_calendar(request, context).await },
    );
    let routes = routes.tool::<UpsertEpochRequest, _, _>(
        "Publish a version of a mission epoch for the tenant. Repeating a call with the same idempotency key returns the same version.",
        policy(EpochsWrite),
        |domain, request, context| async move { domain.create_epoch(request, context).await },
    );
    let routes = routes.tool::<ReplaceClockQualityPolicyRequest, _, _>(
        "Replace the tenant's clock quality policy, which assess_clock applies when a call passes none. Pass the record version you last read, or none when no policy exists yet.",
        policy(ClockPoliciesWrite),
        |domain, request, context| async move { domain.replace_clock_policy(request, context).await },
    );
    let mut routes = resources::register(routes);
    for prompt in TimePrompt::ALL {
        routes = routes.prompt(
            prompt.definition(),
            policy(AuthoritiesRead),
            move |_, request, _| async move { prompt.render(request.arguments) },
        );
    }
    routes.build()
});

impl DomainServer for TimeMcp {
    type Contract = TimeContract;

    fn setup() -> &'static McpServerSetup<TimeContract> {
        &SERVER_SETUP
    }

    fn routes() -> &'static Routes<Self> {
        &ROUTES
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        use crate::catalog::TimeCompletion;

        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = require_permission(&context, requirements::SERVER)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let domain = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::CALENDAR_TEMPLATE, "calendar_id") => Some(TimeCompletion::CalendarId),
            (uris::CALENDAR_TEMPLATE, "version") => Some(TimeCompletion::CalendarVersion {
                calendar_key: request
                    .context
                    .as_ref()
                    .and_then(|context| context.get_argument("calendar_id"))
                    .map(|key| crate::contract::CalendarId::parse(key.clone()))
                    .transpose()
                    .map_err(invalid_params)?,
            }),
            (uris::EPOCH_TEMPLATE | uris::EPOCH_VERSION_TEMPLATE, "epoch_id") => {
                Some(TimeCompletion::EpochId)
            }
            (uris::EPOCH_VERSION_TEMPLATE, "version") => Some(TimeCompletion::EpochVersion {
                epoch_key: request
                    .context
                    .as_ref()
                    .and_then(|context| context.get_argument("epoch_id"))
                    .map(|key| crate::EpochId::parse(key.clone()))
                    .transpose()
                    .map_err(invalid_params)?,
            }),
            (uris::AUTHORITY_RELEASE_TEMPLATE, "release_id") => {
                Some(TimeCompletion::AuthorityReleaseId)
            }
            (uris::EVENT_TEMPLATE, "event_id") => Some(TimeCompletion::EventId),
            _ => None,
        };
        if let Some(domain) = domain {
            // One value beyond a page shows that more matches exist.
            let values = self
                .state
                .catalog
                .complete_values(
                    &scope,
                    domain,
                    &request.argument.value,
                    u32::try_from(CompletionInfo::MAX_VALUES + 1).unwrap_or(u32::MAX),
                )
                .await
                .map_err(internal)?;
            return completion(values);
        }
        // These catalogs are packaged with the server's TZDB and bootstrap data.
        let mut values: Vec<String> = match (reference.uri.as_str(), request.argument.name.as_str())
        {
            (uris::BOOTSTRAP_AUTHORITY_TEMPLATE, "release_id") => self
                .state
                .authorities
                .bootstrap_references()
                .into_iter()
                .map(|reference| reference.release_id().to_string())
                .collect(),
            (uris::ZONE_TEMPLATE, "zone_id") => self
                .state
                .engine(&scope)
                .await
                .map_err(internal)?
                .authority()
                .tzdb()
                .available()
                .map(|name| name.to_string())
                .collect(),
            _ => Vec::new(),
        };
        values.sort();
        values.dedup();
        completion(rank_completions(
            values.iter().map(String::as_str),
            &request.argument.value,
        ))
    }
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}
fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
fn not_found(kind: &str) -> McpError {
    McpError::resource_not_found(format!("unknown {kind}"), None)
}
fn default_clock_policy() -> ClockQualityPolicy {
    ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(100_000_000)
        .maximum_stratum(4)
        .minimum_source_diversity(2)
        .maximum_holdover_seconds(300)
        .build()
        .expect("valid built-in Time clock policy")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_resources_and_simple_templates_agree_with_typed_addresses() {
        use crate::contract::TimeResource;
        use veoveo_types::ResourceAddress;
        for resource in SERVER_SETUP
            .resources()
            .iter()
            .map(|resource| resource.descriptor())
        {
            let address = TimeResource::parse(&resource.uri).unwrap();
            assert_eq!(address.to_uri().unwrap().as_str(), resource.uri);
        }
        for (template, wire) in [
            (uris::DOC_TEMPLATE, "time://docs/agents"),
            (
                uris::AUTHORITY_RELEASE_TEMPLATE,
                "time://authorities/releases/time-release-one",
            ),
            (
                uris::CALENDAR_TEMPLATE,
                "time://calendars/calendar-one/versions/12",
            ),
            (uris::EPOCH_TEMPLATE, "time://epochs/epoch-one"),
            (uris::EVENT_TEMPLATE, "time://events/event-one"),
        ] {
            let address = TimeResource::parse(wire).unwrap();
            assert!(
                veoveo_types::ResourceUriTemplate::new(template)
                    .unwrap()
                    .matches_uri(&address.to_uri().unwrap())
            );
        }
        let conventions = veoveo_hub_sdk::ServerResourceUris::new(
            veoveo_types::ResourceScheme::parse("time").expect("declared resource scheme"),
        );
        assert_eq!(uris::DOCS_URI, conventions.docs_root_uri());
        assert_eq!(uris::CONTRACT_URI, conventions.contract_uri().as_str());
        assert_eq!(uris::DOC_TEMPLATE, conventions.doc_template());
    }

    #[test]
    fn discovery_uses_static_roots_and_templates_without_list_change_notifications() {
        let capabilities = SERVER_SETUP
            .server_config()
            .capabilities
            .resources
            .as_ref()
            .unwrap();
        assert!(!capabilities.list_changed.unwrap_or(false));
        let roots: Vec<_> = SERVER_SETUP
            .resources()
            .iter()
            .map(|resource| resource.descriptor())
            .collect();
        for root in [uris::CALENDARS_URI, uris::EPOCHS_URI, uris::EVENTS_URI] {
            assert!(roots.iter().any(|resource| resource.uri == root));
        }
        // Collection templates derive the same shared selection as resource dispatch.
        let templates = SERVER_SETUP.resource_templates();
        for root in [uris::CALENDARS_URI, uris::EPOCHS_URI, uris::EVENTS_URI] {
            let expected = format!("{root}{{?limit,cursor}}");
            assert!(
                templates
                    .iter()
                    .any(|resource| resource.template().as_str() == expected)
            );
        }
    }
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_hub_sdk::docs::{ContractId, DOC_ID_AGENTS, DOC_ID_DESIGN};

    use super::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "time");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Purpose"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(agents)"));
        assert!(index.contains("(design)"));
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration = veoveo_hub_sdk::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server().as_str(), "time");
        assert_eq!(declaration.contract(), ContractId::HostedMcp);
        let json = serde_json::to_value(&declaration).expect("declaration serializes");
        assert_eq!(json["server"], "time");
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_hub_sdk::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
