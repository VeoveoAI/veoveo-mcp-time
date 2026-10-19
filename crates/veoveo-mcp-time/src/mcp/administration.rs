//! Tenant administration as MCP tools: authority sources, acquisitions, release
//! activation, calendar and epoch publication, and the clock quality policy. Each tool
//! is bound to its permission by the ready routes; reads and listings require
//! `time.authorities.inspect`.
use super::{TimeMcp, internal};
use crate::contract::*;
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_hub_sdk::{
    definition::List,
    hosting::{ToolError, hub_identity},
};

impl TimeMcp {
    pub(super) async fn list_sources(
        &self,
        request: List<ListSources>,
        context: RequestContext<RoleServer>,
    ) -> Result<veoveo_types::Page<Sources>, McpError> {
        let identity = hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .sources_page(&scope, request.selection())
            .await
        {
            Ok(page) => Ok(page),
            Err(error) => Err(crate::index::query_error(error)),
        }
    }

    pub(super) async fn get_source(
        &self,
        request: GetSourceRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<TimeSource, ToolError> {
        let identity = hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self.state.catalog.source(&scope, &request.source_id).await {
            Ok(Some(source)) => Ok(source),
            Ok(None) => missing("time source"),
            Err(error) => rejected(error),
        }
    }

    pub(super) async fn create_source(
        &self,
        request: CreateSourceRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<TimeSource, ToolError> {
        let identity = hub_identity(&context)?;
        let request = CreateSourceRequestValue::from(request);
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .create_source(&scope, request.source)
            .await
        {
            Ok(source) => Ok(source),
            Err(error) => rejected(error),
        }
    }

    pub(super) async fn replace_source(
        &self,
        request: ReplaceSourceRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<TimeSource, ToolError> {
        let identity = hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .replace_source(&scope, request.source, request.expected_record_version)
            .await
        {
            Ok(source) => Ok(source),
            Err(error) => rejected(error),
        }
    }

    pub(super) async fn list_acquisitions(
        &self,
        request: List<ListAcquisitions>,
        context: RequestContext<RoleServer>,
    ) -> Result<veoveo_types::Page<Acquisitions>, McpError> {
        let identity = hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .acquisitions_page(&scope, request.selection())
            .await
        {
            Ok(page) => Ok(page),
            Err(error) => Err(crate::index::query_error(error)),
        }
    }

    pub(super) async fn get_acquisition(
        &self,
        request: GetAcquisitionRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<TimeAcquisition, ToolError> {
        let identity = hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .catalog
            .acquisition(&scope, &request.acquisition_id)
            .await
        {
            Ok(Some(acquisition)) => Ok(acquisition),
            Ok(None) => missing("time acquisition"),
            Err(error) => rejected(error),
        }
    }

    pub(super) async fn start_acquisition(
        &self,
        request: StartAcquisitionRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<TimeAcquisition, ToolError> {
        let identity = hub_identity(&context)?;
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
            Ok(acquisition) => Ok(acquisition),
            Err(error) => rejected(error),
        }
    }

    pub(super) async fn cancel_acquisition(
        &self,
        request: CancelAcquisitionRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<TimeAcquisition, ToolError> {
        let identity = hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match self
            .state
            .acquisitions
            .cancel(&scope, &request.acquisition_id)
            .await
        {
            Ok(acquisition) => Ok(acquisition),
            Err(error) => rejected(error),
        }
    }

    pub(super) async fn list_active_authorities(
        &self,
        request: List<ListActiveAuthorities>,
        context: RequestContext<RoleServer>,
    ) -> Result<veoveo_types::Page<ActiveAuthorities>, McpError> {
        let identity = hub_identity(&context)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        self.state
            .catalog
            .active_authorities_page(&scope, request.selection())
            .await
            .map_err(crate::index::query_error)
    }

    pub(super) async fn activate_release(
        &self,
        request: ActivateReleaseRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<AuthorityRelease, ToolError> {
        let identity = hub_identity(&context)?;
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
        Ok(release)
    }

    pub(super) async fn create_calendar(
        &self,
        request: CreateCalendarRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<OperationalCalendar, ToolError> {
        let identity = hub_identity(&context)?;
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
        Ok(calendar)
    }

    pub(super) async fn create_epoch(
        &self,
        request: UpsertEpochRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<Epoch, ToolError> {
        let identity = hub_identity(&context)?;
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
        Ok(epoch)
    }

    pub(super) async fn replace_clock_policy(
        &self,
        request: ReplaceClockQualityPolicyRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ClockQualityPolicyRecord, ToolError> {
        let identity = hub_identity(&context)?;
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
        Ok(record)
    }
}

/// Preserve actionable domain rejections as typed tool errors.
fn missing<T>(kind: &str) -> Result<T, ToolError> {
    Err(ToolError::Rejected(format!("Unknown {kind}.")))
}
fn rejected<T>(error: anyhow::Error) -> Result<T, ToolError> {
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
        Err(ToolError::Rejected(message))
    } else {
        Err(internal(error).into())
    }
}
