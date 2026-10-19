use super::{
    TimeAccessContext, TimeCatalog, acquisition_from_record, calendar_from_record,
    epoch_from_record, event_from_record, release_from_record, source_from_record,
};
use crate::contract::*;
use anyhow::Result;
use veoveo_hub_sdk::definition::CollectionRequest;
use veoveo_types::{LiveKeyset, Page, Pager};

impl TimeCatalog {
    pub async fn sources_page(
        &self,
        scope: &TimeAccessContext,
        request: &CollectionRequest<Sources>,
    ) -> Result<Page<Sources>> {
        let pager = Pager::<Sources>::new(
            scope.identity.tenant_id,
            *request.filters(),
            request.limit(),
            LiveKeyset,
            request.cursor(),
        )?;
        let rows = self
            .persistence
            .time_sources_page(
                scope.identity.tenant_id,
                pager.after(),
                pager.fetch_limit() as u32,
            )
            .await?;
        Ok(pager.finish(
            rows.into_iter()
                .map(source_from_record)
                .collect::<Result<_>>()?,
        )?)
    }
    pub async fn acquisitions_page(
        &self,
        scope: &TimeAccessContext,
        request: &CollectionRequest<Acquisitions>,
    ) -> Result<Page<Acquisitions>> {
        let pager = Pager::<Acquisitions>::new(
            scope.identity.tenant_id,
            *request.filters(),
            request.limit(),
            LiveKeyset,
            request.cursor(),
        )?;
        let rows = self
            .persistence
            .time_acquisitions_page(
                scope.identity.tenant_id,
                pager.after(),
                pager.fetch_limit() as u32,
            )
            .await?;
        Ok(pager.finish(
            rows.into_iter()
                .map(acquisition_from_record)
                .collect::<Result<_>>()?,
        )?)
    }
    pub async fn calendars_page(
        &self,
        scope: &TimeAccessContext,
        request: &CollectionRequest<Calendars>,
    ) -> Result<Page<Calendars>> {
        let pager = Pager::<Calendars>::new(
            scope.identity.tenant_id,
            *request.filters(),
            request.limit(),
            LiveKeyset,
            request.cursor(),
        )?;
        let rows = self
            .persistence
            .list_time_calendar_versions(
                scope.identity.tenant_id,
                pager.after(),
                pager.fetch_limit() as u32,
            )
            .await?;
        Ok(pager.finish(
            rows.into_iter()
                .map(calendar_from_record)
                .collect::<Result<_>>()?,
        )?)
    }
    pub async fn epochs_page(
        &self,
        scope: &TimeAccessContext,
        request: &CollectionRequest<Epochs>,
    ) -> Result<Page<Epochs>> {
        let pager = Pager::<Epochs>::new(
            scope.identity.tenant_id,
            *request.filters(),
            request.limit(),
            LiveKeyset,
            request.cursor(),
        )?;
        let rows = self
            .persistence
            .list_time_epochs(
                scope.identity.tenant_id,
                pager.after(),
                pager.fetch_limit() as u32,
            )
            .await?;
        Ok(pager.finish(
            rows.into_iter()
                .map(epoch_from_record)
                .collect::<Result<_>>()?,
        )?)
    }
    pub async fn events_page(
        &self,
        scope: &TimeAccessContext,
        request: &CollectionRequest<Events>,
    ) -> Result<Page<Events>> {
        let policy = EventKeyset::new(veoveo_types::PrincipalId::parse(
            scope.identity.principal_id.to_string(),
        )?);
        let pager = Pager::<Events>::new(
            scope.identity.tenant_id,
            request.filters().clone(),
            request.limit(),
            policy,
            request.cursor(),
        )?;
        let rows = self
            .persistence
            .list_time_events(
                &scope.identity,
                pager.after(),
                pager.filters().state.map(super::event_state),
                pager.fetch_limit() as u32,
            )
            .await?;
        Ok(pager.finish(
            rows.into_iter()
                .map(event_from_record)
                .collect::<Result<_>>()?,
        )?)
    }
    pub async fn releases_page(
        &self,
        scope: &TimeAccessContext,
        request: &CollectionRequest<Releases>,
    ) -> Result<Page<Releases>> {
        let pager = Pager::<Releases>::new(
            scope.identity.tenant_id,
            *request.filters(),
            request.limit(),
            LiveKeyset,
            request.cursor(),
        )?;
        let rows = self
            .persistence
            .time_authority_releases_page(
                scope.identity.tenant_id,
                pager.after(),
                pager.fetch_limit() as u32,
            )
            .await?;
        Ok(pager.finish(
            rows.into_iter()
                .map(|row| release_from_record(row).map(ReleaseEntry::from))
                .collect::<Result<_>>()?,
        )?)
    }
    pub async fn active_authorities_page(
        &self,
        scope: &TimeAccessContext,
        request: &CollectionRequest<ActiveAuthorities>,
    ) -> Result<Page<ActiveAuthorities>> {
        let pager = Pager::<ActiveAuthorities>::new(
            scope.identity.tenant_id,
            *request.filters(),
            request.limit(),
            LiveKeyset,
            request.cursor(),
        )?;
        Ok(pager.select(self.active_authorities(scope).await?)?)
    }
    pub async fn epochs_for_keys(
        &self,
        scope: &TimeAccessContext,
        keys: &[EpochId],
    ) -> Result<Vec<Epoch>> {
        anyhow::ensure!(
            keys.len() <= 100_000,
            "at most 100000 epoch keys are supported"
        );
        let mut epochs = Vec::new();
        for keys in keys.chunks(100) {
            let rows = self
                .persistence
                .latest_time_epochs(scope.identity.tenant_id, keys)
                .await?;
            for row in rows {
                epochs.push(epoch_from_record(row)?);
            }
        }
        Ok(epochs)
    }
}
