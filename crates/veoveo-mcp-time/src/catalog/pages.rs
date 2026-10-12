use super::{
    TimeAccessContext, TimeCatalog, calendar_from_record, epoch_from_record, event_from_record,
};
use crate::contract::EventState;
use crate::{
    contract::{
        CalendarCursor, CollectionPage, Epoch, EpochCursor, EpochId, Event, EventCursor,
        OperationalCalendar, TimeVersion,
    },
    index,
};
use anyhow::Result;

impl TimeCatalog {
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
