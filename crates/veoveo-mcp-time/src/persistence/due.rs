//! Scheduled events become due when the database clock reaches their `due` instant.
use super::*;

/// The earliest scheduled due instant and the database clock that read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NextDue {
    pub(crate) due: Option<DateTime<Utc>>,
    pub(crate) now: DateTime<Utc>,
}

impl NextDue {
    /// How long to wait for the next event: zero when it is already due, none when no
    /// event is scheduled.
    pub(crate) fn wait(&self) -> Option<std::time::Duration> {
        self.due
            .map(|due| (due - self.now).to_std().unwrap_or_default())
    }
}

/// A scheduled event that became due.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DueEvent {
    pub(crate) tenant: TenantId,
    pub(crate) event_key: EventId,
}

impl TimePersistence {
    /// Move every scheduled event whose instant has passed to due, once across replicas.
    pub(crate) async fn mark_due_events(&self) -> Result<Vec<DueEvent>, PersistenceError> {
        Ok(
            sqlx::query_file_as!(DueEvent, "src/persistence/queries/mark_due_events.sql")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub(crate) async fn next_due_event(&self) -> Result<NextDue, PersistenceError> {
        let row = sqlx::query_file!("src/persistence/queries/next_due_event.sql")
            .fetch_one(&self.pool)
            .await?;
        Ok(NextDue {
            due: row.next_due,
            now: row.now,
        })
    }
}
