//! Recovery through Time's Task consumer: a replica that claimed a schedule Task dies or
//! stops mid-calculation, its instance's recovery returns the Task to the queue, and
//! another replica's consumer resumes it to the same result. A cancellation requested on
//! another replica ends the calculation at its next checkpoint.
use super::*;
use crate::{
    contract::{
        CalendarId, CalendarWindowValue, ExpandScheduleOutput, ExpandScheduleRequest,
        OperationalCalendarValue, RecurrenceFrequency, RecurrenceRuleValue, ResolveTimeRequest,
        TimeExpressionValue, TimeInstant, TimeVersion, TimeWindow,
    },
    engine::TemporalEngine,
    registry::tests::AuthorityFiles,
};

fn instant(engine: &TemporalEngine, value: &str) -> TimeInstant {
    engine
        .resolve(&ResolveTimeRequest {
            expression: TimeExpressionValue::Rfc3339 {
                value: value.into(),
            }
            .build()
            .unwrap(),
            additional_uncertainty_nanoseconds: 0,
        })
        .unwrap()
        .into_instant()
}

/// A daily 09:00–10:00 UTC window across three days of `engine`'s authority.
fn schedule(engine: &TemporalEngine) -> ExpandScheduleRequest {
    ExpandScheduleRequest {
        calendar: OperationalCalendarValue {
            calendar_id: CalendarId::parse("calendar-recovery-test").unwrap(),
            version: TimeVersion::FIRST,
            name: "Recovery shifts".into(),
            zone_id: "UTC".into(),
            windows: vec![
                CalendarWindowValue {
                    start_local: "2026-01-01T09:00:00".into(),
                    end_local: "2026-01-01T10:00:00".into(),
                    recurrence: RecurrenceRuleValue {
                        frequency: RecurrenceFrequency::Daily,
                        interval: 1,
                        weekdays: vec![],
                        count: None,
                        until: None,
                    }
                    .build()
                    .unwrap(),
                    labels: vec!["shift".into()],
                }
                .build()
                .unwrap(),
            ],
            excluded_dates: vec![],
        }
        .build()
        .unwrap(),
        horizon: TimeWindow::new(
            instant(engine, "2026-01-01T00:00:00Z"),
            instant(engine, "2026-01-04T00:00:00Z"),
        )
        .unwrap(),
        maximum_occurrences: 10,
    }
}

#[tokio::test]
async fn a_replica_resumes_a_schedule_task_whose_instance_died_mid_work() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let doomed = files.replica(&db.a).await;
        let scope = doomed.scope_from_task_owner(&owner()).await.unwrap();
        let engine = doomed.engine(&scope).await.unwrap();
        let request = schedule(&engine);
        let expected = engine.expand_schedule(&request).unwrap();
        let starts: Vec<i64> = expected
            .occurrences
            .iter()
            .map(|occurrence| occurrence.window.start().tai_seconds_since_1970)
            .collect();
        let daily = [
            "2026-01-01T09:00:00Z",
            "2026-01-02T09:00:00Z",
            "2026-01-03T09:00:00Z",
        ]
        .map(|value| instant(&engine, value).tai_seconds_since_1970);
        assert_eq!(starts, daily);
        let request = TimeTaskRequest::ExpandSchedule(Box::new(request));
        let queued = doomed
            .tasks
            .create(time_task(&doomed.tasks, owner(), &request, BTreeSet::new()).unwrap())
            .await
            .unwrap()
            .snapshot;
        let task_id = queued.task_id;
        assert_eq!(queued.recovery_class, RecoveryClass::Resume);

        // Authority reads wait behind this lock, so the doomed replica's worker claims the
        // Task, reports progress and then waits inside its calculation's authority load.
        let mut hold = db.b.pool().begin().await.unwrap();
        sqlx::query_file!("src/tests/queries/hold_authority_reads.sql")
            .execute(&mut *hold)
            .await
            .unwrap();
        let doomed_stop = CancellationToken::new();
        let doomed_work = tokio::spawn(work(doomed.clone(), doomed_stop.clone()));
        while sqlx::query_file_scalar!("src/tests/queries/held_authority_readers.sql")
            .fetch_one(db.b.pool())
            .await
            .unwrap()
            == 0
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let working = doomed.tasks.get(task_id).await.unwrap().unwrap();
        assert_eq!(working.status, TaskStatus::Running);
        assert_eq!(working.owner_instance, Some(doomed.tasks.instance().id()));
        assert_eq!(
            working.status_message.as_deref(),
            Some("expanding operational calendar")
        );
        assert!(working.result.is_none());

        // The doomed process's instance lock ends as its crash would, and the kernel
        // reaper returns its resumable Task to the queue.
        let terminated = sqlx::query_file_scalar!("src/tests/queries/end_instance_lock.sql")
            .fetch_all(db.a.pool())
            .await
            .unwrap();
        assert_eq!(terminated, [true], "one session held the instance lock");
        let instance = doomed.tasks.instance().id();
        while db.b.instance_alive(instance).await.unwrap() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        db.b.reap_instances().await.unwrap();
        let requeued = doomed.tasks.get(task_id).await.unwrap().unwrap();
        assert_eq!(requeued.status, TaskStatus::Queued);
        assert_eq!(requeued.owner_instance, None);
        assert_eq!(requeued.started_at, None);
        assert_eq!(requeued.progress, 0.0);
        assert!(requeued.revision > working.revision);
        assert_eq!(requeued.request, queued.request);

        // The released worker of the dead replica no longer owns the Task, so it settles
        // nothing. Its consumer ends once the instance is gone, without claiming again.
        hold.rollback().await.unwrap();
        let ended = tokio::time::timeout(Duration::from_secs(30), doomed_work)
            .await
            .expect("the dead replica's Task consumer kept running")
            .unwrap();
        assert!(
            ended.is_err(),
            "the dead replica's Task consumer ended cleanly"
        );
        doomed_stop.cancel();
        let unclaimed = doomed.tasks.get(task_id).await.unwrap().unwrap();
        assert_eq!(unclaimed.status, TaskStatus::Queued);
        assert_eq!(unclaimed.owner_instance, None);

        // A replacement replica's consumer claims the Task and completes the calculation.
        let survivor = files.replica(&db.b).await;
        let survivor_stop = CancellationToken::new();
        let survivor_work = tokio::spawn(work(survivor.clone(), survivor_stop.clone()));
        let settled = loop {
            let current = survivor.tasks.get(task_id).await.unwrap().unwrap();
            if current.is_terminal() {
                break current;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(settled.status, TaskStatus::Succeeded, "{:?}", settled.error);
        assert!(settled.revision > unclaimed.revision);
        assert!(settled.started_at.is_some());
        let result = settled.result.unwrap();
        assert_eq!(result["isError"], serde_json::json!(false));
        let output: ExpandScheduleOutput =
            serde_json::from_value(result["structuredContent"].clone()).unwrap();
        assert_eq!(output, expected);
        survivor_stop.cancel();
        survivor_work.await.unwrap().unwrap();
        survivor.tasks.instance().shutdown().await.unwrap();
    })
    .await
    .expect("Time Task restart recovery exceeded 90 seconds");
}

/// A schedule Task claimed by `replica`'s consumer and held inside its authority load
/// until `hold` ends.
struct Held {
    task_id: TaskId,
    expected: ExpandScheduleOutput,
    hold: sqlx::Transaction<'static, sqlx::Postgres>,
    stop: CancellationToken,
    work: tokio::task::JoinHandle<anyhow::Result<()>>,
}

async fn held(db: &TimeDb, replica: &Arc<TimeApplication>) -> Held {
    let scope = replica.scope_from_task_owner(&owner()).await.unwrap();
    let engine = replica.engine(&scope).await.unwrap();
    let request = schedule(&engine);
    let expected = engine.expand_schedule(&request).unwrap();
    let request = TimeTaskRequest::ExpandSchedule(Box::new(request));
    let task_id = replica
        .tasks
        .create(time_task(&replica.tasks, owner(), &request, BTreeSet::new()).unwrap())
        .await
        .unwrap()
        .snapshot
        .task_id;
    let mut hold = db.b.pool().begin().await.unwrap();
    sqlx::query_file!("src/tests/queries/hold_authority_reads.sql")
        .execute(&mut *hold)
        .await
        .unwrap();
    let stop = CancellationToken::new();
    let work = tokio::spawn(work(replica.clone(), stop.clone()));
    while sqlx::query_file_scalar!("src/tests/queries/held_authority_readers.sql")
        .fetch_one(db.b.pool())
        .await
        .unwrap()
        == 0
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let working = replica.tasks.get(task_id).await.unwrap().unwrap();
    assert_eq!(working.status, TaskStatus::Running);
    assert_eq!(working.owner_instance, Some(replica.tasks.instance().id()));
    Held {
        task_id,
        expected,
        hold,
        stop,
        work,
    }
}

async fn terminal(runtime: &TaskRuntime, task_id: TaskId) -> TaskSnapshot {
    loop {
        let current = runtime.get(task_id).await.unwrap().unwrap();
        if current.is_terminal() {
            return current;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_stopping_consumer_leaves_its_running_schedule_task_for_another_replica() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let stopping = files.replica(&db.a).await;
        let held = held(&db, &stopping).await;

        // The process stops while the calculation waits for its authority. Its consumer
        // waits for the calculation, which publishes nothing and invents no cancellation.
        held.stop.cancel();
        held.hold.rollback().await.unwrap();
        tokio::time::timeout(Duration::from_secs(30), held.work)
            .await
            .expect("the stopping consumer never drained")
            .unwrap()
            .unwrap();
        let left = stopping.tasks.get(held.task_id).await.unwrap().unwrap();
        assert_eq!(left.status, TaskStatus::Running);
        assert_eq!(left.owner_instance, Some(stopping.tasks.instance().id()));
        assert!(left.cancel_requested_at.is_none());
        assert!(left.result.is_none());

        // The process ends; recovery queues the Task and another replica finishes it.
        stopping.tasks.instance().shutdown().await.unwrap();
        let survivor = files.replica(&db.b).await;
        let survivor_stop = CancellationToken::new();
        let survivor_work = tokio::spawn(work(survivor.clone(), survivor_stop.clone()));
        let settled = terminal(&survivor.tasks, held.task_id).await;
        assert_eq!(settled.status, TaskStatus::Succeeded, "{:?}", settled.error);
        let output: ExpandScheduleOutput =
            serde_json::from_value(settled.result.unwrap()["structuredContent"].clone()).unwrap();
        assert_eq!(output, held.expected);
        survivor_stop.cancel();
        survivor_work.await.unwrap().unwrap();
        survivor.tasks.instance().shutdown().await.unwrap();
    })
    .await
    .expect("Time Task stop recovery exceeded 90 seconds");
}

#[tokio::test]
async fn a_cancellation_from_another_replica_ends_a_running_schedule_task() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let files = AuthorityFiles::new().await;
        let replica = files.replica(&db.a).await;
        let held = held(&db, &replica).await;

        // Another replica records the cancellation; this consumer's worker reads it at
        // its next checkpoint, once the authority load returns.
        let other = runtime(&db.b).await;
        let requested = other.cancel(held.task_id).await.unwrap();
        assert_eq!(requested.status, TaskStatus::CancelRequested);
        held.hold.rollback().await.unwrap();
        let settled = terminal(&other, held.task_id).await;
        assert_eq!(settled.status, TaskStatus::Cancelled);
        assert!(settled.result.is_none());
        held.stop.cancel();
        held.work.await.unwrap().unwrap();
        replica.tasks.instance().shutdown().await.unwrap();
    })
    .await
    .expect("Time Task remote cancellation exceeded 90 seconds");
}
