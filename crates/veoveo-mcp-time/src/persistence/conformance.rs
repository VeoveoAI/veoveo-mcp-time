//! Time's queries and its `time` schema against the server schema rules of core's
//! `docs/DATABASE.md`: tenant predicates, row bounds, index use under generic plans,
//! and primary keys, foreign-key indexes and retention comments.
//!
//! The rules govern the statements Time runs, which all live under `src/persistence`.
//! The native fixture statements in `src/tests/queries` corrupt rows and inspect
//! sessions on purpose, and the EXPLAIN copies there repeat production statements.
use veoveo_fixtures::database::Conformance;

#[tokio::test]
async fn queries_and_schema_follow_the_database_rules() {
    Conformance::new()
        .server(&crate::changes::server(), super::migrator())
        .queries_from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.sqlx"))
        .sources(concat!(env!("CARGO_MANIFEST_DIR"), "/src/persistence"))
        .check()
        .await;
}
