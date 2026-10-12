//! The due loop. One per process reads the earliest scheduled due instant, sleeps until it
//! or until an event changes, then moves every event whose instant has passed to due.
//! Replicas race on the same rows and the database changes each one once; the row
//! triggers then notify every replica's subscribers.
use std::{sync::Arc, time::Duration};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::catalog::TimeCatalog;

/// How long the loop waits after a database error before it tries again.
const RETRY_DELAY: Duration = Duration::from_secs(5);

/// Run until `stop` is cancelled. `wake` interrupts the wait when an event changes or the
/// change listener reconnects.
pub async fn run_due_events(catalog: TimeCatalog, wake: Arc<Notify>, stop: CancellationToken) {
    loop {
        let wait = match due_round(&catalog).await {
            Ok(wait) => wait,
            Err(error) => {
                tracing::warn!(%error, "Time's due loop failed; retrying");
                Some(RETRY_DELAY)
            }
        };
        let sleep = async {
            match wait {
                Some(wait) => tokio::time::sleep(wait).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = stop.cancelled() => return,
            () = wake.notified() => {}
            () = sleep => {}
        }
    }
}

/// Mark what is due, then return how long until the next scheduled event.
async fn due_round(catalog: &TimeCatalog) -> anyhow::Result<Option<Duration>> {
    let due = catalog.mark_due_events().await?;
    if !due.is_empty() {
        tracing::debug!(events = due.len(), "Time events became due");
    }
    Ok(catalog.next_due_event().await?.wait())
}
