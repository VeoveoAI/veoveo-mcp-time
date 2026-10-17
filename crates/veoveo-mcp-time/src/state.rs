use std::{collections::BTreeSet, sync::Arc, time::Duration};

use anyhow::Result;
use veoveo_hub_sdk::{HubInternalIdentity, PrincipalKind};
use veoveo_store::{PlatformStore, PrincipalKind as StorePrincipalKind};
use veoveo_task::{TaskOwner, TaskRuntime};

use crate::{
    acquisition::AcquisitionService,
    catalog::{TimeAccessContext, TimeCatalog},
    changes::TenantSubscriptions,
    clock::ClockMonitor,
    engine::TemporalEngine,
    registry::AuthorityRegistry,
};

#[derive(Clone)]
pub struct TimeApplication {
    pub store: PlatformStore,
    pub tasks: TaskRuntime,
    pub catalog: TimeCatalog,
    pub authorities: AuthorityRegistry,
    pub clock: ClockMonitor,
    pub acquisitions: Arc<AcquisitionService>,
    pub subscriptions: Arc<TenantSubscriptions>,
}

impl TimeApplication {
    pub async fn scope(&self, identity: &HubInternalIdentity) -> Result<TimeAccessContext> {
        let tenant_key = identity
            .actor
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        let platform_identity = self
            .store
            .ensure_identity(
                &tenant_key,
                identity.actor.id.as_str(),
                identity.actor.issuer.as_str(),
                identity.actor.subject.as_str(),
                match identity.actor.kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        Ok(TimeAccessContext {
            identity: platform_identity,
            work_context: identity.authority.work_context.clone(),
        })
    }

    pub async fn scope_from_task_owner(&self, owner: &TaskOwner) -> Result<TimeAccessContext> {
        let identity = self
            .store
            .ensure_identity(
                owner.tenant().as_str(),
                owner.principal_key.as_str(),
                owner.issuer.as_str(),
                owner.subject.as_str(),
                owner.principal_kind,
            )
            .await?;
        Ok(TimeAccessContext {
            identity,
            work_context: owner.authority.work_context.clone(),
        })
    }

    pub async fn engine(&self, scope: &TimeAccessContext) -> Result<TemporalEngine> {
        self.authorities
            .authority_engine(&self.catalog, scope)
            .await
    }

    pub async fn engine_for_expressions<'a>(
        &self,
        scope: &TimeAccessContext,
        expressions: impl IntoIterator<Item = &'a crate::contract::TimeExpression>,
    ) -> Result<TemporalEngine> {
        let mut keys = BTreeSet::new();
        for (index, expression) in expressions.into_iter().enumerate() {
            anyhow::ensure!(
                index < 100_000,
                "at most 100000 temporal expressions are supported"
            );
            if let crate::contract::TimeExpressionValue::EpochRelative { epoch_id, .. } =
                &**expression
            {
                keys.insert(epoch_id.clone());
            }
        }
        let keys: Vec<_> = keys.into_iter().collect();
        let engine = self.engine(scope).await?;
        let epochs = tokio::time::timeout(
            Duration::from_secs(30),
            self.catalog.epochs_for_keys(scope, &keys),
        )
        .await??;
        engine.replace_epochs(epochs);
        Ok(engine)
    }
}
