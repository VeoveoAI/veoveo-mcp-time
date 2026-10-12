//! Time resource subscriptions. A listener receives Task updates and the resource changes
//! of its caller's tenant only.

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError,
    model::SubscriptionFilter,
    service::{RequestContext, SubscriptionContext},
};
use veoveo_mcp_contract::hosting::requested_addresses;
use veoveo_task_runtime::{DurableListener, DurableTaskService, listen_durable_subscriptions};

use super::{
    internal,
    requirements::{Target, authorize},
};
use crate::{contract::TimeResource, state::TimeApplication};

/// Admits subscriptions to Time's mutable resources and delivers them from the caller's
/// tenant hub, which the change watcher feeds from the `time` channel.
pub(crate) struct TimeSubscriptions {
    state: Arc<TimeApplication>,
}

impl TimeSubscriptions {
    pub(crate) fn new(state: Arc<TimeApplication>) -> Self {
        Self { state }
    }

    /// Check every requested address for the caller and return the caller's tenant hub,
    /// or none when the request names no resources.
    async fn admit(
        &self,
        addresses: Vec<TimeResource>,
        context: &RequestContext<rmcp::RoleServer>,
    ) -> Result<Option<Arc<veoveo_mcp_contract::SubscriptionHub>>, McpError> {
        if addresses.is_empty() {
            return Ok(None);
        }
        if let Some(immutable) = addresses.iter().find(|address| !address.is_subscribable()) {
            tracing::debug!(?immutable, "rejected Time subscription");
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        let identity = veoveo_mcp_contract::hosting::gateway_identity(context)?;
        for address in &addresses {
            authorize(&identity, Target::Resource(&address.to_string()))?;
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        Ok(Some(self.state.subscriptions.hub(scope.identity.tenant_id)))
    }
}

impl<S: DurableTaskService> DurableListener<S> for TimeSubscriptions {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, service: &S, context: SubscriptionContext) -> Result<(), McpError> {
        let addresses = requested_addresses::<TimeResource>(
            context.accepted().resource_subscriptions.as_deref(),
        )?;
        let hub = self.admit(addresses, context.request_context()).await?;
        listen_durable_subscriptions(service, context, hub.as_deref(), None).await
    }
}
