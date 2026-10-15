//! Plans of the statements whose work could grow with a tenant's rows. Each one seeks an
//! index past its cursor, inside its prefix range or up to the due instant, rather than
//! reading every row of the tenant through a filter.
//!
//! The plans come from the EXPLAIN statements in `src/tests/queries`, which repeat the
//! production statements verbatim. Their parameters are bound, so PostgreSQL plans them
//! with those values; a production statement whose predicates never depend on a missing
//! cursor has the same index conditions in its cached generic plan.
use serde_json::Value;
use veoveo_store::{PrincipalId, PrincipalKind, TenantId};

use crate::test_database::TimeDb;

const EXPLAIN: &str = "EXPLAIN (FORMAT JSON)\n";

#[test]
fn explained_statements_are_the_production_statements() {
    for (name, explained, production) in [
        (
            "source_page_after",
            include_str!("../tests/queries/explain_source_page_after.sql"),
            include_str!("queries/source_page_after.sql"),
        ),
        (
            "acquisition_page_after",
            include_str!("../tests/queries/explain_acquisition_page_after.sql"),
            include_str!("queries/acquisition_page_after.sql"),
        ),
        (
            "release_page_after",
            include_str!("../tests/queries/explain_release_page_after.sql"),
            include_str!("queries/release_page_after.sql"),
        ),
        (
            "calendar_page_after",
            include_str!("../tests/queries/explain_calendar_page_after.sql"),
            include_str!("queries/calendar_page_after.sql"),
        ),
        (
            "epoch_page_after",
            include_str!("../tests/queries/explain_epoch_page_after.sql"),
            include_str!("queries/epoch_page_after.sql"),
        ),
        (
            "event_page_after",
            include_str!("../tests/queries/explain_event_page_after.sql"),
            include_str!("queries/event_page_after.sql"),
        ),
        (
            "complete_calendar_id",
            include_str!("../tests/queries/explain_complete_calendar_id.sql"),
            include_str!("queries/complete_calendar_id.sql"),
        ),
        (
            "complete_event_id",
            include_str!("../tests/queries/explain_complete_event_id.sql"),
            include_str!("queries/complete_event_id.sql"),
        ),
        (
            "mark_due_events",
            include_str!("../tests/queries/explain_mark_due_events.sql"),
            include_str!("queries/mark_due_events.sql"),
        ),
    ] {
        let (_, statement) = explained
            .split_once(EXPLAIN)
            .unwrap_or_else(|| panic!("explain_{name} has no EXPLAIN line"));
        assert_eq!(statement, production, "explain_{name} differs from {name}");
    }
}

/// Every plan node under `node`, depth first.
fn nodes<'a>(node: &'a Value, found: &mut Vec<&'a Value>) {
    found.push(node);
    for child in node["Plans"].as_array().into_iter().flatten() {
        nodes(child, found);
    }
}

/// Fail unless the plan reads no table sequentially and, for each set of `conditions`, an
/// index scan of `relation`, through `index` when one is named, seeks with all of them.
fn seeks(plan: impl Into<Value>, relation: &str, index: Option<&str>, conditions: &[&[&str]]) {
    let plan = plan.into();
    let mut found = Vec::new();
    nodes(&plan[0]["Plan"], &mut found);
    assert!(
        found.iter().all(|node| node["Node Type"] != "Seq Scan"),
        "a sequential scan: {plan:#}"
    );
    for conditions in conditions {
        assert!(
            found.iter().any(|node| {
                node["Relation Name"] == relation
                    && index.is_none_or(|index| node["Index Name"] == index)
                    && node["Index Cond"]
                        .as_str()
                        .is_some_and(|cond| conditions.iter().all(|wanted| cond.contains(wanted)))
            }),
            "no index scan of {relation} seeks with {conditions:?}: {plan:#}"
        );
    }
}

#[tokio::test]
async fn paging_completion_and_due_statements_seek_their_indexes() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let db = TimeDb::new().await;
        let identity =
            db.a.ensure_identity(
                "plans",
                "owner",
                "https://plans.example",
                "owner",
                PrincipalKind::User,
            )
            .await
            .unwrap();
        let tenant = identity.tenant_id;
        let owner = identity.principal_id;
        let mut plans = db.a.pool().begin().await.unwrap();
        sqlx::query_file_scalar!("src/tests/queries/disable_seqscan.sql")
            .fetch_one(&mut *plans)
            .await
            .unwrap();

        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_source_page_after.sql",
            tenant as TenantId,
            "time-source-00000000-0000-7000-8000-000000000000",
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(plan, "source", None, &[&["source_key >"]]);

        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_acquisition_page_after.sql",
            tenant as TenantId,
            "time-acquisition-00000000-0000-7000-8000-000000000000",
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(plan, "acquisition", None, &[&["acquisition_key <"]]);

        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_release_page_after.sql",
            tenant as TenantId,
            "time-release-00000000-0000-7000-8000-000000000000",
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(plan, "authority_release", None, &[&["release_key >"]]);

        // The mixed-direction order seeks the cursor's calendar below its version, then
        // the calendars after it, and the same for epochs.
        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_calendar_page_after.sql",
            tenant as TenantId,
            "calendar-00000000-0000-7000-8000-000000000000",
            7_i64,
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(
            plan,
            "calendar_version",
            None,
            &[
                &["calendar_key =", "calendar_version <"],
                &["calendar_key >"],
            ],
        );

        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_epoch_page_after.sql",
            tenant as TenantId,
            "epoch-00000000-0000-7000-8000-000000000000",
            7_i64,
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(
            plan,
            "epoch",
            None,
            &[&["epoch_key =", "epoch_version <"], &["epoch_key >"]],
        );

        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_event_page_after.sql",
            tenant as TenantId,
            owner as PrincipalId,
            Option::<&str>::None,
            2_000_000_000_i64,
            17_i32,
            "event-00000000-0000-7000-8000-000000000000",
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(
            plan,
            "event",
            Some("event_page"),
            &[&["ROW(due_tai_seconds_since_1970, due_nanosecond, event_key) >"]],
        );

        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_complete_calendar_id.sql",
            tenant as TenantId,
            "calendar-0",
            "calendar-1",
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(
            plan,
            "calendar_version",
            None,
            &[&["calendar_key >=", "calendar_key <"]],
        );

        let plan = sqlx::query_file_scalar!(
            "src/tests/queries/explain_complete_event_id.sql",
            tenant as TenantId,
            owner as PrincipalId,
            "event-0",
            "event-1",
            101_i64,
        )
        .fetch_one(&mut *plans)
        .await
        .unwrap();
        seeks(
            plan,
            "event",
            Some("event_owner_key"),
            &[&["owner =", "event_key >=", "event_key <"]],
        );

        let plan =
            sqlx::query_file_scalar!("src/tests/queries/explain_mark_due_events.sql", 500_i64)
                .fetch_one(&mut *plans)
                .await
                .unwrap();
        seeks(
            plan,
            "event",
            Some("event_scheduled_due"),
            &[&["due <= now()"]],
        );
        plans.rollback().await.unwrap();
    })
    .await
    .expect("Time plan qualification exceeded 90 seconds");
}
