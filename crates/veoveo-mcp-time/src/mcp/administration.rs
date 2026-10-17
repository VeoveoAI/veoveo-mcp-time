//! Tenant administration as MCP tools: authority sources, acquisitions, release
//! activation, calendar and epoch publication, and the clock quality policy. Each tool
//! requires the permission the requirement table names; reads and listings require
//! `time.authorities.inspect`.
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    service::RequestContext,
    tool, tool_router,
};
use veoveo_hub_sdk::hosting::structured_result;

use super::{
    TimeMcp, internal,
    requirements::{Target, require},
};
use crate::contract::{
    AcquisitionCursor, ActivateReleaseRequest, ActiveAuthoritySelection, AdminPage,
    AuthorityRelease, CancelAcquisitionRequest, ClockQualityPolicyRecord, CreateCalendarRequest,
    CreateCalendarRequestValue, CreateSourceRequest, CreateSourceRequestValue, Epoch,
    GetAcquisitionRequest, GetSourceRequest, ListAcquisitionsRequest, ListSourcesRequest, NoCursor,
    OperationalCalendar, ReplaceClockQualityPolicyRequest, ReplaceSourceRequest, SourceCursor,
    StartAcquisitionRequest, StartAcquisitionRequestValue, TimeAcquisition, TimeSource,
    UpsertEpochRequest, UpsertEpochRequestValue,
};

#[tool_router(router = administration_router, vis = "pub(crate)")]
impl TimeMcp {
    #[tool(
        title = "List authority sources",
        description = "List the tenant's registered time-authority sources, the HTTPS locations of IANA time zone and leap-second data, in source ID order. A page holds up to `limit` sources, 100 by default. Pass its `nextCursor` as `cursor` to read the next page.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AdminPage<TimeSource, SourceCursor>>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_sources(
        &self,
        Parameters(request): Parameters<ListSourcesRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("list_sources"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .sources_page(&scope, request.cursor.as_ref(), request.limit)
            .await
        {
            Ok(page) => structured_result(format!("{} sources", page.items.len()), &page),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "Get authority source",
        description = "Read one registered time-authority source.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeSource>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn get_source(
        &self,
        Parameters(request): Parameters<GetSourceRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("get_source"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self.state.catalog.source(&scope, &request.source_id).await {
            Ok(Some(source)) => structured_result(source.source_id.to_string(), &source),
            Ok(None) => missing("time source"),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "Create authority source",
        description = "Register an HTTPS source of IANA time zone or leap-second data for the tenant. Repeating a call with the same idempotency key returns the same source.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeSource>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn create_source(
        &self,
        Parameters(request): Parameters<CreateSourceRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("create_source"))?;
        let request = CreateSourceRequestValue::from(request);
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .create_source(&scope, request.source)
            .await
        {
            Ok(source) => structured_result(format!("registered {}", source.source_id), &source),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "Replace authority source",
        description = "Replace a registered source's settings. Pass the record version you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeSource>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn replace_source(
        &self,
        Parameters(request): Parameters<ReplaceSourceRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("replace_source"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .replace_source(&scope, request.source, request.expected_record_version)
            .await
        {
            Ok(source) => structured_result(format!("replaced {}", source.source_id), &source),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "List acquisitions",
        description = "List the tenant's authority acquisitions and their progress, the most recently started first. A page holds up to `limit` acquisitions, 100 by default. Pass its `nextCursor` as `cursor` to read the next page.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AdminPage<TimeAcquisition, AcquisitionCursor>>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_acquisitions(
        &self,
        Parameters(request): Parameters<ListAcquisitionsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("list_acquisitions"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .acquisitions_page(&scope, request.cursor.as_ref(), request.limit)
            .await
        {
            Ok(page) => structured_result(format!("{} acquisitions", page.items.len()), &page),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "Get acquisition",
        description = "Read one acquisition's status, phase and staged release.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeAcquisition>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn get_acquisition(
        &self,
        Parameters(request): Parameters<GetAcquisitionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("get_acquisition"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .acquisition(&scope, &request.acquisition_id)
            .await
        {
            Ok(Some(acquisition)) => {
                structured_result(acquisition.acquisition_id.to_string(), &acquisition)
            }
            Ok(None) => missing("time acquisition"),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "Start acquisition",
        description = "Download, verify and compile a new release from a registered source, staging it for activation. The acquisition continues after the call returns; read its progress with get_acquisition. Repeating a call with the same idempotency key returns the same acquisition.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeAcquisition>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = true)
    )]
    async fn start_acquisition(
        &self,
        Parameters(request): Parameters<StartAcquisitionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("start_acquisition"))?;
        let request = StartAcquisitionRequestValue::from(request);
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let source = match self.state.catalog.source(&scope, &request.source_id).await {
            Ok(Some(source)) => source,
            Ok(None) => return missing("time source"),
            Err(error) => return rejected(error),
        };
        match self
            .state
            .acquisitions
            .start(
                scope,
                source,
                request.expected_source_digest,
                request.idempotency_key,
            )
            .await
        {
            Ok(acquisition) => structured_result(
                format!(
                    "acquisition {} {:?}",
                    acquisition.acquisition_id, acquisition.status
                ),
                &acquisition,
            ),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "Cancel acquisition",
        description = "Request cancellation of a running acquisition. Its scratch content is removed and no release is staged.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeAcquisition>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn cancel_acquisition(
        &self,
        Parameters(request): Parameters<CancelAcquisitionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("cancel_acquisition"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .acquisitions
            .cancel(&scope, &request.acquisition_id)
            .await
        {
            Ok(acquisition) => structured_result(
                format!(
                    "acquisition {} {:?}",
                    acquisition.acquisition_id, acquisition.status
                ),
                &acquisition,
            ),
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "List active authorities",
        description = "List the tenant's active release for each authority dataset with the pointer version activate_release requires as its expected active pointer version.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AdminPage<ActiveAuthoritySelection, NoCursor>>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_active_authorities(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("list_active_authorities"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self.state.catalog.active_authorities(&scope).await {
            // At most one release per dataset family, so one page holds them all.
            Ok(items) => {
                let page = AdminPage::<_, NoCursor> {
                    items,
                    next_cursor: None,
                };
                structured_result(format!("{} active authorities", page.items.len()), &page)
            }
            Err(error) => rejected(error),
        }
    }

    #[tool(
        title = "Activate release",
        description = "Make a staged release the tenant's active authority for its dataset. The previous release is retired and kept. Pass the release's record version and the active pointer version you last read; the call fails if either has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AuthorityRelease>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn activate_release(
        &self,
        Parameters(request): Parameters<ActivateReleaseRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("activate_release"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let candidate = match self
            .state
            .catalog
            .release(&scope, &request.release_id)
            .await
        {
            Ok(Some(release)) => release,
            Ok(None) => return missing("authority release"),
            Err(error) => return rejected(error),
        };
        let release = match self
            .state
            .authorities
            .activate_release(
                &self.state.catalog,
                &scope,
                &candidate.release_id,
                request.expected_release_record_version,
                request.expected_active_pointer_version,
            )
            .await
        {
            Ok(release) => release,
            Err(error) => return rejected(error),
        };
        self.state
            .authorities
            .reload(&self.state.catalog, &scope)
            .await
            .map_err(internal)?;
        structured_result(format!("activated {}", release.release_id), &release)
    }

    #[tool(
        title = "Create calendar version",
        description = "Publish a version of an operational calendar for the tenant. Repeating a call with the same idempotency key returns the same version.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<OperationalCalendar>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn create_calendar(
        &self,
        Parameters(request): Parameters<CreateCalendarRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("create_calendar"))?;
        let request = CreateCalendarRequestValue::from(request);
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let calendar = match self
            .state
            .catalog
            .create_calendar(&scope, request.calendar)
            .await
        {
            Ok(calendar) => calendar,
            Err(error) => return rejected(error),
        };
        structured_result(format!("published {}", calendar.calendar_id), &calendar)
    }

    #[tool(
        title = "Create epoch version",
        description = "Publish a version of a mission epoch for the tenant. Repeating a call with the same idempotency key returns the same version.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Epoch>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn create_epoch(
        &self,
        Parameters(request): Parameters<UpsertEpochRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("create_epoch"))?;
        let request = UpsertEpochRequestValue::from(request);
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let epoch = match self.state.catalog.create_epoch(&scope, request.epoch).await {
            Ok(epoch) => epoch,
            Err(error) => return rejected(error),
        };
        self.state
            .authorities
            .reload(&self.state.catalog, &scope)
            .await
            .map_err(internal)?;
        structured_result(format!("published {}", epoch.epoch_id), &epoch)
    }

    #[tool(
        title = "Replace clock policy",
        description = "Replace the tenant's clock quality policy, which assess_clock applies when a call passes none. Pass the record version you last read, or none when no policy exists yet.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ClockQualityPolicyRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn replace_clock_policy(
        &self,
        Parameters(request): Parameters<ReplaceClockQualityPolicyRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require(&context, Target::Tool("replace_clock_policy"))?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let (policy, record_version) = match self
            .state
            .catalog
            .replace_clock_policy(&scope, request.policy, request.expected_record_version)
            .await
        {
            Ok(result) => result,
            Err(error) => return rejected(error),
        };
        let record = ClockQualityPolicyRecord {
            policy,
            record_version,
        };
        structured_result("replaced clock policy".to_owned(), &record)
    }
}

/// A tool execution error the caller can act on, reported in the result as MCP
/// specifies for tool errors.
fn failed(message: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(
        message.into(),
    )]))
}

fn missing(kind: &str) -> Result<CallToolResult, McpError> {
    failed(format!("Unknown {kind}."))
}

/// Report a domain rejection, such as a version conflict or an invalid value, in the
/// tool result; anything else is an internal error.
fn rejected(error: anyhow::Error) -> Result<CallToolResult, McpError> {
    let message = error.to_string();
    let actionable = [
        "unknown",
        "disappeared",
        "conflict",
        "record_version",
        "invalid",
        "must",
        "disabled",
        "digest",
    ]
    .iter()
    .any(|marker| message.contains(marker));
    if actionable {
        failed(message)
    } else {
        Err(internal(error))
    }
}
