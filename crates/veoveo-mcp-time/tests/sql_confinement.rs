//! Time's SQL names only the `time` schema: its queries, migrations and test queries. Core
//! state, such as Tasks, process instances, usage and audit, reaches Time only through
//! the SDK and core's runtime API.

#[test]
fn sql_names_only_the_time_schema() {
    veoveo_hub_sdk::testing::assert_sql_confined(env!("CARGO_MANIFEST_DIR"), "time");
}
