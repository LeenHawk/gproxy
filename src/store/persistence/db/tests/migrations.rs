//! Schema creation, migration, and legacy-table repair tests.

use super::*;
use crate::store::persistence::traits::{CorePersistence, ProviderPersistence, RoutingPersistence};

#[tokio::test]
async fn sqlite_memory_connect_and_health() {
    mem().await.health().await.expect("health");
}

#[tokio::test]
async fn connect_stamps_latest_and_leaves_nothing_pending() {
    use crate::store::persistence::migrations::latest_version;
    use sea_orm::{ConnectionTrait, Statement};

    let db = mem().await;
    let backend = db.conn.get_database_backend();
    let row = db
        .conn
        .query_one_raw(Statement::from_string(
            backend,
            "SELECT COALESCE(MAX(version), 0) AS v FROM schema_migrations".to_string(),
        ))
        .await
        .expect("query")
        .expect("row");
    let max = row.try_get::<i64>("", "v").expect("v");

    // Fresh connect creates the current schema and stamps the latest listed
    // version directly — replaying a migration (e.g. ADD COLUMN) against the
    // just-created tables would fail.
    assert_eq!(max, latest_version());
}

#[tokio::test]
async fn migrates_cache_breakpoint_message_target() {
    use sea_orm::{ConnectionTrait, Database, Statement};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-cache-target.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE rules (\
            id INTEGER PRIMARY KEY, \
            rule_set_id INTEGER NOT NULL, \
            kind TEXT NOT NULL, \
            config_json TEXT NOT NULL, \
            filter_model_pattern TEXT, \
            filter_operation_keys TEXT, \
            sort_order INTEGER NOT NULL, \
            enabled INTEGER NOT NULL, \
            created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old rules table");
    conn.execute_unprepared(
        "INSERT INTO rules \
            (id, rule_set_id, kind, config_json, sort_order, enabled, created_at, updated_at) \
         VALUES \
            (1, 1, 'cache_breakpoint', \
             '{\"index\":-1, \"target\": \"last_message\", \"ttl\":\"5m\"}', 0, 1, 0, 0)",
    )
    .await
    .expect("old cache breakpoint rule");
    conn.execute_unprepared(crate::store::persistence::migrations::CREATE_MIGRATIONS_TABLE)
        .await
        .expect("schema_migrations");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (10, 0)")
        .await
        .expect("version 10");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let backend = db.conn.get_database_backend();
    let row = db
        .conn
        .query_one_raw(Statement::from_string(
            backend,
            "SELECT config_json FROM rules WHERE id = 1".to_owned(),
        ))
        .await
        .expect("query")
        .expect("rule row");
    let config = serde_json::from_str::<serde_json::Value>(
        &row.try_get::<String>("", "config_json")
            .expect("config_json"),
    )
    .expect("valid config JSON");
    assert_eq!(config["target"], "message");
    assert_eq!(config["index"], -1);
    assert_eq!(config["ttl"], "5m");
}

#[tokio::test]
async fn migrates_old_alias_table_to_scoped_aliases() {
    use sea_orm::{ConnectionTrait, Database, Statement};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE aliases (\
            id INTEGER PRIMARY KEY, \
            alias TEXT NOT NULL UNIQUE, \
            route_id INTEGER NOT NULL, \
            created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old aliases table");
    conn.execute_unprepared(
        "INSERT INTO aliases (id, alias, route_id, created_at, updated_at) \
         VALUES (1, 'old-alias', 9, 10, 11)",
    )
    .await
    .expect("old alias row");
    // A faithful pre-v7 DB also predates the cache-token columns (migration 7).
    // Seed usage_rollups WITHOUT them so create_all's IF NOT EXISTS leaves it
    // alone and migration 7's ADD COLUMN applies — as on a real v5→latest
    // upgrade. Only the dimension columns the unique index needs are required.
    conn.execute_unprepared(
        "CREATE TABLE usage_rollups (\
            id INTEGER PRIMARY KEY, \
            granularity TEXT NOT NULL, \
            bucket_start INTEGER NOT NULL, \
            provider_id INTEGER, \
            org_id INTEGER, \
            team_id INTEGER, \
            user_id INTEGER, \
            route_name TEXT, \
            model TEXT)",
    )
    .await
    .expect("old usage_rollups table");
    conn.execute_unprepared(crate::store::persistence::migrations::CREATE_MIGRATIONS_TABLE)
        .await
        .expect("schema_migrations");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (5, 0)")
        .await
        .expect("version 5");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let aliases = db.list_aliases().await.expect("aliases");
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0].provider, "*");
    assert_eq!(aliases[0].alias, "old-alias");
    assert_eq!(aliases[0].target, "old-alias");

    let backend = db.conn.get_database_backend();
    let cols = db
        .conn
        .query_all_raw(Statement::from_string(
            backend,
            "PRAGMA table_info(aliases)".to_owned(),
        ))
        .await
        .expect("table_info")
        .into_iter()
        .map(|row| row.try_get::<String>("", "name").expect("column name"))
        .collect::<Vec<_>>();
    assert!(!cols.iter().any(|col| col == "route_id"));

    db.upsert_alias(AliasInput {
        id: None,
        provider: "p1".to_owned(),
        alias: "same".to_owned(),
        target: Some("m1".to_owned()),
        sort_order: 0,
        enabled: true,
    })
    .await
    .expect("p1 alias");
    db.upsert_alias(AliasInput {
        id: None,
        provider: "p2".to_owned(),
        alias: "same".to_owned(),
        target: Some("m2".to_owned()),
        sort_order: 0,
        enabled: true,
    })
    .await
    .expect("p2 alias");
}

#[tokio::test]
async fn repairs_old_price_rules_rates_json_table() {
    use crate::store::persistence::migrations::{CREATE_MIGRATIONS_TABLE, latest_version};
    use rust_decimal::Decimal;
    use sea_orm::{ConnectionTrait, Database, Statement};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-pricing.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE price_rules (\
            id INTEGER PRIMARY KEY, \
            provider_id INTEGER, \
            match_type TEXT NOT NULL, \
            model_match TEXT NOT NULL, \
            operation TEXT, \
            kind TEXT, \
            rates_json TEXT NOT NULL, \
            priority INTEGER NOT NULL, \
            enabled INTEGER NOT NULL, \
            created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old price_rules table");
    conn.execute_unprepared(
        "INSERT INTO price_rules \
         (id, provider_id, match_type, model_match, operation, kind, rates_json, priority, enabled, created_at, updated_at) \
         VALUES \
         (1, 7, 'exact', 'gpt-test', NULL, NULL, \
          '{\"input_tokens\":\"0.40\",\"output_tokens\":\"1.60\",\"cache_read_tokens\":\"0.10\",\"cache_write_tokens\":\"2.50\",\"image_count\":\"0.04\"}', \
          3, 1, 10, 11)",
    )
    .await
    .expect("old price rule row");
    conn.execute_unprepared(CREATE_MIGRATIONS_TABLE)
        .await
        .expect("schema_migrations");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (8, 0)")
        .await
        .expect("version 8");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("repair");
    let rules = db.list_price_rules().await.expect("price rules");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].input_price, Decimal::new(40, 2));
    assert_eq!(rules[0].output_price, Decimal::new(160, 2));
    assert_eq!(rules[0].cache_read_price, Decimal::new(10, 2));
    assert_eq!(rules[0].cache_creation_5m_price, Decimal::new(250, 2));
    assert_eq!(rules[0].cache_creation_1h_price, Decimal::new(250, 2));
    assert_eq!(rules[0].image_price, Decimal::new(4, 2));

    let backend = db.conn.get_database_backend();
    let cols = db
        .conn
        .query_all_raw(Statement::from_string(
            backend,
            "PRAGMA table_info(price_rules)".to_string(),
        ))
        .await
        .expect("columns")
        .into_iter()
        .map(|row| row.try_get::<String>("", "name"))
        .collect::<Result<Vec<_>, _>>()
        .expect("column names");
    assert!(!cols.iter().any(|col| col == "rates_json"));
    assert!(!cols.iter().any(|col| col == "cache_write_price"));
    assert!(!cols.iter().any(|col| col == "operation"));
    assert!(!cols.iter().any(|col| col == "kind"));
    assert!(!cols.iter().any(|col| col == "priority"));

    db.upsert_price_rule(PriceRuleInput {
        id: None,
        provider_id: None,
        match_type: "contains".into(),
        model_match: "new-model".into(),
        input_price: Decimal::new(1, 0),
        output_price: Decimal::new(2, 0),
        cache_read_price: Decimal::ZERO,
        cache_creation_5m_price: Decimal::ZERO,
        cache_creation_30m_price: Decimal::ZERO,
        cache_creation_1h_price: Decimal::ZERO,
        image_price: Decimal::ZERO,
        enabled: true,
    })
    .await
    .expect("insert repaired price rule");
    assert_eq!(db.list_price_rules().await.expect("after insert").len(), 2);

    let row = db
        .conn
        .query_one_raw(Statement::from_string(
            backend,
            "SELECT COALESCE(MAX(version), 0) AS v FROM schema_migrations".to_string(),
        ))
        .await
        .expect("query")
        .expect("row");
    assert_eq!(row.try_get::<i64>("", "v").expect("v"), latest_version());
}
