use super::{
    TimeAccessContext, TimeCatalog, acquisition_from_record, calendar_from_record,
    epoch_from_record, event_from_record, release_from_record, source_from_record,
};
use crate::contract::EventState;
use crate::{
    contract::{
        AcquisitionCursor, AdminPage, AdminPageLimit, AuthorityCursor, AuthorityRelease,
        CalendarCursor, CollectionPage, Epoch, EpochCursor, EpochId, Event, EventCursor,
        OperationalCalendar, SourceCursor, TimeAcquisition, TimeSource, TimeVersion,
    },
    index,
};
use anyhow::Result;

impl TimeCatalog {
    /// One page of the tenant's sources in source ID order.
    pub async fn sources_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&SourceCursor>,
        limit: AdminPageLimit,
    ) -> Result<AdminPage<TimeSource, SourceCursor>> {
        let rows = self
            .persistence
            .time_sources_page(scope.identity.tenant_id, after, index::admin_fetch(limit))
            .await?;
        index::admin_page(
            rows,
            limit,
            |row| Ok(SourceCursor::new(&row.source_key)),
            source_from_record,
        )
    }

    /// One page of the tenant's acquisitions, the most recently started first.
    pub async fn acquisitions_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&AcquisitionCursor>,
        limit: AdminPageLimit,
    ) -> Result<AdminPage<TimeAcquisition, AcquisitionCursor>> {
        let rows = self
            .persistence
            .time_acquisitions_page(scope.identity.tenant_id, after, index::admin_fetch(limit))
            .await?;
        index::admin_page(
            rows,
            limit,
            |row| Ok(AcquisitionCursor::new(&row.acquisition_key)),
            acquisition_from_record,
        )
    }

    pub async fn calendars_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&CalendarCursor>,
    ) -> Result<CollectionPage<OperationalCalendar, CalendarCursor>> {
        let rows = self
            .persistence
            .list_time_calendar_versions(scope.identity.tenant_id, after, 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(CalendarCursor::new(
                    &row.calendar_key,
                    TimeVersion::new(row.calendar_version.try_into()?)?,
                ))
            },
            calendar_from_record,
        )
    }
    pub async fn epochs_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&EpochCursor>,
    ) -> Result<CollectionPage<Epoch, EpochCursor>> {
        let rows = self
            .persistence
            .list_time_epochs(scope.identity.tenant_id, after, 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(EpochCursor::new(
                    &row.epoch_key,
                    TimeVersion::new(row.epoch_version.try_into()?)?,
                ))
            },
            epoch_from_record,
        )
    }
    pub async fn events_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&EventCursor>,
        state: Option<EventState>,
    ) -> Result<CollectionPage<Event, EventCursor>> {
        let rows = self
            .persistence
            .list_time_events(&scope.identity, after, state.map(super::event_state), 101)
            .await?;
        index::page(
            rows,
            |row| {
                Ok(EventCursor::new(
                    &row.event_key,
                    row.due_tai_seconds_since_1970,
                    crate::SubsecondNanoseconds::new(row.due_nanosecond.try_into()?)?,
                ))
            },
            event_from_record,
        )
    }
    pub async fn releases_page(
        &self,
        scope: &TimeAccessContext,
        after: Option<&AuthorityCursor>,
    ) -> Result<CollectionPage<AuthorityRelease, AuthorityCursor>> {
        let rows = self
            .persistence
            .time_authority_releases_page(scope.identity.tenant_id, after, 101)
            .await?;
        index::page(
            rows,
            |row| Ok(AuthorityCursor::new(&row.release_key)),
            release_from_record,
        )
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
