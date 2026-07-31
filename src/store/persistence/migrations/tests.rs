use super::*;

#[test]
fn pending_filters_and_orders_by_version() {
    let all = pending(BASELINE_VERSION);
    assert_eq!(
        all.iter().map(|m| m.version).collect::<Vec<_>>(),
        MIGRATIONS.iter().map(|m| m.version).collect::<Vec<_>>(),
    );
    let mut prev = BASELINE_VERSION;
    for m in &all {
        assert!(m.version > prev, "versions must strictly ascend");
        assert!(m.version > BASELINE_VERSION, "must be above baseline");
        prev = m.version;
    }
    let top = MIGRATIONS
        .iter()
        .map(|m| m.version)
        .max()
        .unwrap_or(BASELINE_VERSION);
    assert_eq!(latest_version(), top);
    assert!(pending(latest_version()).is_empty());
    assert!(MIN_COMPATIBLE_DATA_VERSION <= latest_version());
}

#[test]
fn postgres_max_version_query_widens_legacy_integer_tables() {
    assert_eq!(
        select_max_version_sql(MigrationDialect::Postgres),
        "SELECT CAST(COALESCE(MAX(version), 0) AS BIGINT) AS v FROM schema_migrations"
    );
    assert_eq!(
        select_max_version_sql(MigrationDialect::Sqlite),
        SELECT_MAX_VERSION
    );
    assert_eq!(
        select_max_version_sql(MigrationDialect::MySql),
        SELECT_MAX_VERSION
    );
}
