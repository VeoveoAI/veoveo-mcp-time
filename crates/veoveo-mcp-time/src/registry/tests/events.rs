//! Durable events become due through the due loop and notify their tenant's subscribers.
use super::{hosted::Fixture, *};
use crate::{
    changes::{ChangeWatcher, channel},
    due::run_due_events,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use veoveo_sdk_rust::ResourceUpdate;

#[tokio::test]
async fn future_cancelled_and_overdue_events_reach_due_through_the_due_loop() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let fixture = Fixture::new().await;
        let state = &fixture.state;
        let owner = scope(&fixture.db.a, "event-owner").await;
        let engine = state.engine(&owner).await.unwrap();
        let now = engine
            .resolve(&ResolveTimeRequest {
                expression: TimeExpressionValue::Rfc3339 {
                    value: Utc::now().to_rfc3339(),
                }
                .build()
                .unwrap(),
                additional_uncertainty_nanoseconds: 0,
            })
            .unwrap()
            .into_instant();
        let hub = state.subscriptions.hub(owner.identity.tenant_id);
        let mut updates = hub.listen();
        // Events 0 and 2 become due, event 1 is cancelled before its due time, and event 3
        // falls due after event 1. A pass that moves event 3 has passed event 1's due time,
        // so event 3 reading Due shows the loop left the cancelled event alone.
        let mut events = Vec::new();
        for (index, offset) in [
            1_000_000_000i128,
            1_500_000_000,
            -1_000_000_000,
            2_000_000_000,
        ]
        .into_iter()
        .enumerate()
        {
            let due = TimeInstant::from_total_nanoseconds(
                now.total_nanoseconds() + offset,
                0,
                now.authority.clone(),
            )
            .unwrap();
            events.push(
                state
                    .catalog
                    .create_event(
                        &owner,
                        Event {
                            event_id: EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap(),
                            name: format!("scheduler native {index}"),
                            due: due.clone(),
                            state: EventState::Scheduled,
                            record_version: TimeVersion::FIRST,
                        },
                        engine.utc_due(&due).unwrap(),
                        format!("native-{index}"),
                    )
                    .await
                    .unwrap(),
            );
        }
        let mut replay = events[0].clone();
        replay.event_id = EventId::parse(format!("event-{}", Uuid::now_v7())).unwrap();
        let replay_due = engine.utc_due(&replay.due).unwrap();
        assert_eq!(
            state
                .catalog
                .create_event(&owner, replay.clone(), replay_due, "native-0".into())
                .await
                .unwrap(),
            events[0]
        );
        replay.name.push_str(" conflicting payload");
        assert!(
            state
                .catalog
                .create_event(&owner, replay, replay_due, "native-0".into())
                .await
                .is_err()
        );
        let cancelled = state
            .catalog
            .cancel_event(&owner, &events[1].event_id, events[1].record_version)
            .await
            .unwrap();
        assert!(
            state
                .catalog
                .cancel_event(&owner, &events[1].event_id, events[1].record_version)
                .await
                .is_err()
        );

        // The process starts its watcher and due loop; nothing reads or subscribes to
        // the events individually.
        let wake = Arc::new(Notify::new());
        let stop = CancellationToken::new();
        let watching = tokio::spawn(
            ChangeWatcher {
                subscriptions: state.subscriptions.clone(),
                authorities: state.authorities.clone(),
                due: wake.clone(),
            }
            .run(fixture.db.b.listen(vec![channel()])),
        );
        let due_loop = tokio::spawn(run_due_events(state.catalog.clone(), wake, stop.clone()));
        // A subscriber rereads what an update names. The watcher's reconciliation names
        // everything, which covers the overdue event that changed before it subscribed.
        let mut due_ids = std::collections::BTreeSet::new();
        while due_ids.len() != 3 {
            let reread = match updates.recv().await.unwrap() {
                ResourceUpdate::Reconcile => true,
                ResourceUpdate::Uri(uri) => {
                    uri == crate::uris::EVENTS_URI
                        || events
                            .iter()
                            .any(|event| uri == format!("time://events/{}", event.event_id))
                }
            };
            if !reread {
                continue;
            }
            for event in [&events[0], &events[2], &events[3]] {
                let current = state
                    .catalog
                    .event(&owner, &event.event_id)
                    .await
                    .unwrap()
                    .unwrap();
                if current.state == EventState::Due {
                    assert_eq!(current.name, event.name);
                    assert_eq!(current.due, event.due);
                    assert_eq!(
                        current.record_version,
                        event.record_version.checked_next().unwrap()
                    );
                    due_ids.insert(current.event_id.to_string());
                }
            }
        }
        // Event 3 is Due, so a pass ran after event 1's due time and kept it cancelled.
        assert_eq!(
            state
                .catalog
                .event(&owner, &cancelled.event_id)
                .await
                .unwrap()
                .unwrap(),
            cancelled
        );
        let foreign = scope(&fixture.db.a, "another-event-owner").await;
        assert!(
            state
                .catalog
                .event(&foreign, &events[0].event_id)
                .await
                .unwrap()
                .is_none()
        );
        stop.cancel();
        watching.abort();
        due_loop.await.unwrap();
    })
    .await
    .expect("native event qualification exceeded thirty seconds");
}
