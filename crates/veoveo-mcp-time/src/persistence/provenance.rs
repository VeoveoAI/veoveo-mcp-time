//! A record's creating principal and Work Context, checked against its owner columns.
use super::*;
use veoveo_mcp_ext_knowledge::{AccessDescriptor, ReadPolicy};
use veoveo_types::AccessSubject;

impl Provenance<'_> {
    /// The access descriptor of a knowledge observation. The stored keys must derive the
    /// row's tenant and owner identifiers.
    pub(crate) fn access(&self, policy: ReadPolicy) -> anyhow::Result<AccessDescriptor> {
        let invalid = || anyhow::anyhow!("invalid stored Time provenance");
        anyhow::ensure!(
            veoveo_platform_store::deterministic_tenant_id(self.tenant_key)? == self.tenant
                && veoveo_platform_store::deterministic_principal_id(
                    self.tenant_key,
                    self.owner_key
                )? == self.owner,
            "stored Time provenance disagrees with ownership"
        );
        Ok(AccessDescriptor {
            expires_at: None,
            tenant: self.tenant_key.parse().map_err(|_| invalid())?,
            work_context: self.work_context.parse().map_err(|_| invalid())?,
            read_policy: policy,
            owner: AccessSubject::Principal(
                veoveo_types::PrincipalId::parse(self.owner_key).map_err(|_| invalid())?,
            ),
            grants: vec![],
            data_labels: vec![],
        })
    }
}
