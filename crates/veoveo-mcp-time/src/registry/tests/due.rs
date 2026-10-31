//! Scheduled events become due from the database clock, without a read or a
//! subscription, and once across replicas.
use super::*;
use crate::{changes::channel, due::run_due_events};
use futures::StreamExt;
use std::collections::BTreeSet;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use veoveo_store::{ChangeOperation, ChangeSignal, ChangeStream};

const WAIT: Duration = Duration::from_secs(10);

async fn event_at(
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    engine: &TemporalEngine,
    offset: chrono::TimeDelta,
) -> Event {
    let due = engine
        .resolve(&ResolveTimeRequest {
            expression: crate::TimeExpressionValue::Rfc3339 {
                value: (Utc::now() + offset).to_rfc3339(),
            }
            .build()
            .unwrap(),
            additional_uncertainty: crate::NanosecondBound::ZERO,
        })
        .unwrap()
        .into_instant();
    let utc = engine.utc_due(&due).unwrap();
    catalog
        .create_event(
            scope,
            Event {
                event_id: EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap(),
                name: "due fixture".into(),
                due,
                state: EventState::Scheduled,
                record_version: TimeVersion::FIRST,
            },
            utc,
            Uuid::now_v7().to_string(),
        )
        .await
        .unwrap()
}

/// Wait until every event in `keys` has reported an update on `changes`.
async fn updated(changes: &mut ChangeStream, keys: &BTreeSet<String>) {
    let mut seen = BTreeSet::new();
    tokio::time::timeout(WAIT, async {
        while !keys.is_subset(&seen) {
            if let ChangeSignal::Changed(change) = changes.next().await.unwrap().unwrap()
                && change.table == "event"
                && change.operation == ChangeOperation::Update
                && let Some(key) = change.key
            {
                seen.insert(key);
            }
        }
    })
    .await
    .expect("every due event notified its change");
}

fn running_loop(catalog: TimeCatalog) -> (Arc<Notify>, CancellationToken) {
    let wake = Arc::new(Notify::new());
    let stop = CancellationToken::new();
    tokio::spawn(run_due_events(catalog, wake.clone(), stop.clone()));
    (wake, stop)
}

#[tokio::test]
async fn overdue_events_become_due_after_a_restart_without_a_read_or_subscription() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let engine = TemporalEngine::new(files.bootstrap.clone());
        let catalog = TimeCatalog::new(db.a.pool().clone());
        let owner = scope(&db.a, "due-restart").await;
        // These rows outlive the process that created them: no loop runs yet.
        let overdue = event_at(&catalog, &owner, &engine, -chrono::TimeDelta::seconds(5)).await;
        let cancelled = event_at(&catalog, &owner, &engine, -chrono::TimeDelta::seconds(5)).await;
        let cancelled = catalog
            .cancel_event(&owner, &cancelled.event_id, TimeVersion::FIRST)
            .await
            .unwrap();
        let future = event_at(&catalog, &owner, &engine, chrono::TimeDelta::days(1)).await;

        let mut changes = db.a.listen(vec![channel()]);
        assert_eq!(
            tokio::time::timeout(WAIT, changes.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            ChangeSignal::Reconcile
        );
        // The restarted process starts its due loop and serves nothing else.
        let (_wake, stop) = running_loop(TimeCatalog::new(db.b.pool().clone()));
        updated(
            &mut changes,
            &BTreeSet::from([overdue.event_id.to_string()]),
        )
        .await;
        stop.cancel();

        let due = catalog
            .event(&owner, &overdue.event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(due.state, EventState::Due);
        assert_eq!(due.record_version.get(), 2);
        assert_eq!(due.due, overdue.due);
        let stored = body(
            &db.a,
            owner.identity.tenant_id,
            Row::Event(&overdue.event_id),
        )
        .await;
        assert_eq!(stored["state"], "due");
        assert_eq!(stored["recordVersion"], 2);
        assert_eq!(
            catalog.event(&owner, &cancelled.event_id).await.unwrap(),
            Some(cancelled)
        );
        assert_eq!(
            catalog.event(&owner, &future.event_id).await.unwrap(),
            Some(future)
        );
    })
    .await
    .expect("due restart qualification exceeded 90 seconds");
}

#[tokio::test]
async fn replicas_change_each_due_event_once() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let engine = TemporalEngine::new(files.bootstrap.clone());
        let left = TimeCatalog::new(db.a.pool().clone());
        let right = TimeCatalog::new(db.b.pool().clone());
        let owner = scope(&db.a, "due-replicas").await;
        let other = scope(&db.a, "due-replicas-other").await;
        let mut overdue = BTreeSet::new();
        for index in 0..20 {
            let scope = if index % 2 == 0 { &owner } else { &other };
            let event = event_at(&left, scope, &engine, -chrono::TimeDelta::seconds(1)).await;
            overdue.insert(event.event_id.to_string());
        }
        let (from_left, from_right) = tokio::join!(left.mark_due_events(), right.mark_due_events());
        let from_left: BTreeSet<_> = from_left
            .unwrap()
            .into_iter()
            .map(|due| due.event_key.to_string())
            .collect();
        let from_right: BTreeSet<_> = from_right
            .unwrap()
            .into_iter()
            .map(|due| due.event_key.to_string())
            .collect();
        assert!(from_left.is_disjoint(&from_right));
        assert_eq!(&from_left | &from_right, overdue);
        assert!(left.mark_due_events().await.unwrap().is_empty());

        // Two running loops wake for the same future events; each changes once.
        let mut changes = db.a.listen(vec![channel()]);
        assert_eq!(
            tokio::time::timeout(WAIT, changes.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            ChangeSignal::Reconcile
        );
        let (_left_wake, left_stop) = running_loop(left.clone());
        let (_right_wake, right_stop) = running_loop(right.clone());
        let mut upcoming = Vec::new();
        for _ in 0..4 {
            upcoming.push(
                event_at(
                    &left,
                    &owner,
                    &engine,
                    chrono::TimeDelta::milliseconds(1500),
                )
                .await,
            );
        }
        let created_at = std::time::Instant::now();
        let keys = upcoming
            .iter()
            .map(|event| event.event_id.to_string())
            .collect::<BTreeSet<_>>();
        updated(&mut changes, &keys).await;
        assert!(created_at.elapsed() >= Duration::from_millis(500));
        // Both loops have had their chance at every row.
        tokio::time::sleep(Duration::from_millis(500)).await;
        left_stop.cancel();
        right_stop.cancel();
        for event in &upcoming {
            let current = left.event(&owner, &event.event_id).await.unwrap().unwrap();
            assert_eq!(current.state, EventState::Due);
            assert_eq!(current.record_version.get(), 2);
        }
    })
    .await
    .expect("due replica qualification exceeded 90 seconds");
}

#[tokio::test]
async fn a_backlog_larger_than_a_batch_becomes_due_in_short_statements() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let engine = TemporalEngine::new(files.bootstrap.clone());
        let catalog = TimeCatalog::new(db.a.pool().clone());
        let owner = scope(&db.a, "due-backlog").await;
        // Seven events passed while no process ran, the earliest first.
        let mut backlog = Vec::new();
        for seconds in (4..=10).rev() {
            let event = event_at(
                &catalog,
                &owner,
                &engine,
                -chrono::TimeDelta::seconds(seconds),
            )
            .await;
            backlog.push(event.event_id.to_string());
        }
        let batch = crate::persistence::DueBatch::of(3);
        let keys = |due: Vec<crate::persistence::DueEvent>| -> BTreeSet<String> {
            due.into_iter()
                .map(|due| due.event_key.to_string())
                .collect()
        };
        // One statement changes one batch, and it takes the earliest events.
        let first = crate::persistence::TimePersistence::new(db.b.pool().clone())
            .mark_due_batch(batch)
            .await
            .unwrap();
        assert_eq!(
            keys(first),
            backlog[..3].iter().cloned().collect::<BTreeSet<_>>()
        );
        // The loop runs full batches until one comes back short.
        let rest = TimeCatalog::new(db.b.pool().clone())
            .mark_due_events_in(batch)
            .await
            .unwrap();
        assert_eq!(
            keys(rest),
            backlog[3..].iter().cloned().collect::<BTreeSet<_>>()
        );
        assert!(catalog.mark_due_events_in(batch).await.unwrap().is_empty());
        for key in &backlog {
            let event = catalog
                .event(&owner, &EventId::parse(key).unwrap())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(event.state, EventState::Due);
            assert_eq!(event.record_version.get(), 2);
        }

        // Replicas running small batches at once still change each event once.
        let mut overdue = BTreeSet::new();
        for _ in 0..20 {
            let event = event_at(&catalog, &owner, &engine, -chrono::TimeDelta::seconds(1)).await;
            overdue.insert(event.event_id.to_string());
        }
        let left = TimeCatalog::new(db.a.pool().clone());
        let right = TimeCatalog::new(db.b.pool().clone());
        let small = crate::persistence::DueBatch::of(4);
        let (from_left, from_right) = tokio::join!(
            left.mark_due_events_in(small),
            right.mark_due_events_in(small)
        );
        let (from_left, from_right) = (keys(from_left.unwrap()), keys(from_right.unwrap()));
        assert!(from_left.is_disjoint(&from_right));
        assert_eq!(&from_left | &from_right, overdue);
    })
    .await
    .expect("due backlog qualification exceeded 90 seconds");
}

#[test]
fn due_instants_round_up_to_the_microsecond() {
    let engine = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { TemporalEngine::new(AuthorityFiles::new().await.bootstrap.clone()) });
    let mut instant = resolved(&engine);
    let whole = engine.utc_due(&instant).unwrap();
    assert_eq!(whole.timestamp_subsec_nanos(), 0);
    instant.nanosecond = SubsecondNanoseconds::new(1).unwrap();
    let due = engine.utc_due(&instant).unwrap();
    assert_eq!(due - whole, chrono::TimeDelta::microseconds(1));
    instant.nanosecond = SubsecondNanoseconds::MAX;
    let due = engine.utc_due(&instant).unwrap();
    assert_eq!(due - whole, chrono::TimeDelta::seconds(1));
}
