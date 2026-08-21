//! Schema creation, migration, and legacy-table repair tests.

use super::*;
use crate::store::persistence::traits::{
    CorePersistence, ProviderPersistence, RoutingPersistence, SettingsPersistence, UsagePersistence,
};

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
async fn repairs_provider_model_limit_columns_without_migration_stamp() {
    use sea_orm::{ConnectionTrait, Database};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-provider-models.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE provider_models (            id INTEGER PRIMARY KEY, provider_id INTEGER NOT NULL, model_id TEXT NOT NULL,             display_name TEXT, variants_json TEXT, enabled INTEGER NOT NULL,             created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old provider_models table");
    conn.execute_unprepared(
        "INSERT INTO provider_models          (id, provider_id, model_id, display_name, variants_json, enabled, created_at, updated_at)          VALUES (1, 7, 'legacy-model', NULL, NULL, 1, 0, 0)",
    )
    .await
    .expect("old provider model row");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("repair");
    let models = db.list_provider_models(7).await.expect("models readable");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model_id, "legacy-model");
    assert_eq!(models[0].context_window, None);
    assert_eq!(models[0].max_output_tokens, None);
    assert_eq!(models[0].thinking_supported, None);
    assert_eq!(models[0].thinking_adaptive_supported, None);
    assert_eq!(models[0].thinking_enabled_supported, None);
}

#[tokio::test]
async fn migrates_instance_settings_optional_columns() {
    use sea_orm::{ConnectionTrait, Database};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-instance-settings.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE instance_settings (\
            id INTEGER PRIMARY KEY, instance_name TEXT NOT NULL UNIQUE, proxy TEXT, \
            spoof_emulation INTEGER, enable_usage INTEGER NOT NULL, \
            enable_upstream_log INTEGER NOT NULL, enable_upstream_log_body INTEGER NOT NULL, \
            enable_downstream_log INTEGER NOT NULL, enable_downstream_log_body INTEGER NOT NULL, \
            disable_log_redaction INTEGER NOT NULL, enable_tokenizer_download INTEGER NOT NULL, \
            update_channel TEXT, retention_days INTEGER, created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old instance_settings table");
    conn.execute_unprepared(
        "INSERT INTO instance_settings (id, instance_name, enable_usage, enable_upstream_log, \
            enable_upstream_log_body, enable_downstream_log, enable_downstream_log_body, \
            disable_log_redaction, enable_tokenizer_download, created_at, updated_at) \
         VALUES (1, 'legacy', 1, 0, 0, 0, 0, 0, 0, 0, 0)",
    )
    .await
    .expect("old instance settings row");
    conn.execute_unprepared(crate::store::persistence::migrations::CREATE_MIGRATIONS_TABLE)
        .await
        .unwrap();
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (12, 0)")
        .await
        .unwrap();
    conn.close().await.unwrap();

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let settings = db
        .list_instance_settings()
        .await
        .expect("new column readable");
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].max_database_size_mb, None);
    assert!(!settings[0].enable_auto_update_check);
}

#[tokio::test]
async fn migrates_magic_cache_setting_by_target_protocol() {
    use sea_orm::{ConnectionTrait, Database, Statement};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-magic-cache.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE providers (\
            id INTEGER PRIMARY KEY, channel TEXT NOT NULL, enabled INTEGER NOT NULL, \
            settings_json TEXT NOT NULL)",
    )
    .await
    .expect("old providers table");
    conn.execute_unprepared(
        "INSERT INTO providers (id, channel, enabled, settings_json) VALUES \
            (1, 'openai', 1, '{\"enable_magic_cache\":true,\"base_url\":\"https://one.example\"}'), \
            (2, 'openai', 1, '{\"enable_magic_cache\":false}'), \
            (3, 'openai', 1, '{\"base_url\":\"https://three.example\"}')",
    )
    .await
    .expect("old provider settings");
    conn.execute_unprepared(crate::store::persistence::migrations::CREATE_MIGRATIONS_TABLE)
        .await
        .expect("schema_migrations");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (13, 0)")
        .await
        .expect("version 13");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let backend = db.conn.get_database_backend();
    let rows = db
        .conn
        .query_all_raw(Statement::from_string(
            backend,
            "SELECT id, settings_json FROM providers ORDER BY id".to_owned(),
        ))
        .await
        .expect("query providers");
    let settings = rows
        .into_iter()
        .map(|row| {
            serde_json::from_str::<serde_json::Value>(
                &row.try_get::<String>("", "settings_json")
                    .expect("settings_json"),
            )
            .expect("valid settings JSON")
        })
        .collect::<Vec<_>>();

    assert_eq!(settings[0]["enable_openai_magic_cache"], true);
    assert_eq!(settings[0]["enable_claude_magic_cache"], true);
    assert_eq!(settings[0]["base_url"], "https://one.example");
    assert!(
        settings
            .iter()
            .all(|value| value.get("enable_magic_cache").is_none())
    );
    assert!(settings[1].get("enable_openai_magic_cache").is_none());
    assert!(settings[1].get("enable_claude_magic_cache").is_none());
    assert_eq!(settings[2]["base_url"], "https://three.example");
}

#[tokio::test]
async fn disables_removed_chatgpt_providers() {
    use sea_orm::{ConnectionTrait, Database, Statement};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-chatgpt-provider.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE providers (\
            id INTEGER PRIMARY KEY, channel TEXT NOT NULL, enabled INTEGER NOT NULL, \
            settings_json TEXT NOT NULL)",
    )
    .await
    .expect("old providers table");
    conn.execute_unprepared(
        "INSERT INTO providers (id, channel, enabled, settings_json) VALUES \
            (1, 'chatgpt', 1, '{}'), (2, 'openai', 1, '{}')",
    )
    .await
    .expect("old providers");
    conn.execute_unprepared(crate::store::persistence::migrations::CREATE_MIGRATIONS_TABLE)
        .await
        .expect("schema_migrations");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (14, 0)")
        .await
        .expect("version 14");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let backend = db.conn.get_database_backend();
    let rows = db
        .conn
        .query_all_raw(Statement::from_string(
            backend,
            "SELECT channel, enabled FROM providers ORDER BY id".to_owned(),
        ))
        .await
        .expect("query providers");

    assert_eq!(rows[0].try_get::<String>("", "channel").unwrap(), "chatgpt");
    assert_eq!(rows[0].try_get::<i64>("", "enabled").unwrap(), 0);
    assert_eq!(rows[1].try_get::<String>("", "channel").unwrap(), "openai");
    assert_eq!(rows[1].try_get::<i64>("", "enabled").unwrap(), 1);
}

#[tokio::test]
async fn migrates_claude_fable_fallback_setting() {
    use sea_orm::{ConnectionTrait, Database, Statement};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-claude-fallback.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE providers (\
            id INTEGER PRIMARY KEY, channel TEXT NOT NULL, enabled INTEGER NOT NULL, \
            settings_json TEXT NOT NULL)",
    )
    .await
    .expect("old providers table");
    conn.execute_unprepared(
        "INSERT INTO providers (id, channel, enabled, settings_json) VALUES \
            (1, 'claudeapi', 1, '{\"enable_claude_fable_fallback\":true,\"base_url\":\"https://one.example\"}'), \
            (2, 'claudeapi', 1, '{\"enable_claude_fable_fallback\":false}'), \
            (3, 'claudeapi', 1, '{\"base_url\":\"https://three.example\"}'), \
            (4, 'claudeapi', 1, '{\"enable_claude_fable_fallback\":true,\"claude_fable_fallbacks\":\"default\"}')",
    )
    .await
    .expect("old provider settings");
    conn.execute_unprepared(crate::store::persistence::migrations::CREATE_MIGRATIONS_TABLE)
        .await
        .expect("schema_migrations");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (15, 0)")
        .await
        .expect("version 15");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let backend = db.conn.get_database_backend();
    let rows = db
        .conn
        .query_all_raw(Statement::from_string(
            backend,
            "SELECT settings_json FROM providers ORDER BY id".to_owned(),
        ))
        .await
        .expect("query providers");
    let settings = rows
        .into_iter()
        .map(|row| {
            serde_json::from_str::<serde_json::Value>(
                &row.try_get::<String>("", "settings_json")
                    .expect("settings_json"),
            )
            .expect("valid settings JSON")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        settings[0]["claude_fable_fallbacks"],
        json!(["claude-opus-4-8"])
    );
    assert_eq!(settings[0]["base_url"], "https://one.example");
    assert!(settings[1].get("claude_fable_fallbacks").is_none());
    assert_eq!(settings[2]["base_url"], "https://three.example");
    assert_eq!(settings[3]["claude_fable_fallbacks"], "default");
    assert!(
        settings
            .iter()
            .all(|value| value.get("enable_claude_fable_fallback").is_none())
    );
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
    assert_eq!(rules[0].image_output_price, Decimal::ZERO);

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
    assert!(!cols.iter().any(|col| col == "image_price"));
    assert!(cols.iter().any(|col| col == "image_output_price"));
    assert!(cols.iter().any(|col| col == "pricing_tiers_json"));

    let pricing_tiers_json = serde_json::json!([{
        "min_prompt_tokens": 200_000,
        "input_price": "4",
        "output_price": "12"
    }]);
    let inserted = db
        .upsert_price_rule(PriceRuleInput {
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
            image_output_price: Decimal::ZERO,
            pricing_tiers_json: Some(pricing_tiers_json.clone()),
            rates: Vec::new(),
            enabled: true,
        })
        .await
        .expect("insert repaired price rule");
    assert_eq!(
        inserted.pricing_tiers_json,
        Some(pricing_tiers_json.clone())
    );
    assert_eq!(inserted.rates.len(), 7);
    assert_eq!(inserted.rates[0].metric, "input_tokens");
    assert_eq!(inserted.rates[0].unit_size, 1_000_000);
    let rules = db.list_price_rules().await.expect("after insert");
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].pricing_tiers_json, Some(pricing_tiers_json));

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

#[tokio::test]
async fn migrates_image_output_pricing_and_usage_without_reusing_per_image_price() {
    use sea_orm::{ConnectionTrait, Database, Statement};

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old-image-pricing.db");
    let dsn = format!("sqlite://{}?mode=rwc", path.display());
    let conn = Database::connect(&dsn).await.expect("seed connect");
    conn.execute_unprepared(
        "CREATE TABLE price_rules (\
            id INTEGER PRIMARY KEY, provider_id INTEGER, match_type TEXT NOT NULL, \
            model_match TEXT NOT NULL, input_price TEXT NOT NULL, output_price TEXT NOT NULL, \
            cache_read_price TEXT NOT NULL, cache_creation_5m_price TEXT NOT NULL, \
            cache_creation_30m_price TEXT NOT NULL, cache_creation_1h_price TEXT NOT NULL, \
            image_price TEXT NOT NULL, enabled INTEGER NOT NULL, \
            created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old price_rules table");
    conn.execute_unprepared(
        "INSERT INTO price_rules VALUES \
         (1, NULL, 'contains', 'image-model', '1', '2', '0', '0', '0', '0', \
          '0.04', 1, 10, 11)",
    )
    .await
    .expect("old price rule");
    conn.execute_unprepared(
        "CREATE TABLE usages (\
            id INTEGER PRIMARY KEY, request_id TEXT NOT NULL UNIQUE, at INTEGER NOT NULL, \
            route_name TEXT, provider_id INTEGER, credential_id INTEGER, org_id INTEGER, \
            team_id INTEGER, user_id INTEGER, user_key_id INTEGER, operation TEXT NOT NULL, \
            kind TEXT NOT NULL, model TEXT, input_tokens INTEGER NOT NULL, \
            output_tokens INTEGER NOT NULL, cache_read_tokens INTEGER NOT NULL, \
            cache_creation_5m_tokens INTEGER NOT NULL, \
            cache_creation_30m_tokens INTEGER NOT NULL DEFAULT 0, \
            cache_creation_1h_tokens INTEGER NOT NULL, cost TEXT NOT NULL, \
            latency_ms INTEGER NOT NULL DEFAULT 0, usage_source TEXT NOT NULL DEFAULT '', \
            ended TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old usages table");
    conn.execute_unprepared(
        "INSERT INTO usages VALUES \
         (1, 'request-1', 100, NULL, NULL, NULL, NULL, NULL, NULL, NULL, \
          'images', 'openai', 'image-model', 3, 7, 0, 0, 0, 0, '0.1', \
          0, 'upstream', 'complete', 10, 11)",
    )
    .await
    .expect("old usage");
    conn.execute_unprepared(
        "CREATE TABLE usage_rollups (\
            id INTEGER PRIMARY KEY, granularity TEXT NOT NULL, bucket_start INTEGER NOT NULL, \
            provider_id INTEGER, org_id INTEGER, team_id INTEGER, user_id INTEGER, \
            route_name TEXT, model TEXT, requests INTEGER NOT NULL, input_tokens INTEGER NOT NULL, \
            output_tokens INTEGER NOT NULL, cache_write_tokens INTEGER NOT NULL DEFAULT 0, \
            cache_read_tokens INTEGER NOT NULL DEFAULT 0, cost TEXT NOT NULL, \
            created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)",
    )
    .await
    .expect("old usage_rollups table");
    conn.execute_unprepared(
        "INSERT INTO usage_rollups VALUES \
         (1, 'hour', 0, NULL, NULL, NULL, NULL, NULL, 'image-model', 1, 3, 7, 0, 0, \
          '0.1', 10, 11)",
    )
    .await
    .expect("old rollup");
    conn.execute_unprepared(crate::store::persistence::migrations::CREATE_MIGRATIONS_TABLE)
        .await
        .expect("schema_migrations");
    conn.execute_unprepared("INSERT INTO schema_migrations (version, applied_at) VALUES (20, 0)")
        .await
        .expect("version 20");
    conn.close().await.expect("close seed");

    let db = DbPersistence::connect(&dsn).await.expect("migrate");
    let rules = db.list_price_rules().await.expect("price rules");
    assert_eq!(rules[0].image_output_price, rust_decimal::Decimal::ZERO);
    let usages = db.list_usages(10).await.expect("usages");
    assert_eq!(usages[0].output_tokens, 7);
    assert_eq!(usages[0].image_output_tokens, 0);
    let rollups = db
        .list_usage_rollups("hour", 0, 0, None)
        .await
        .expect("rollups");
    assert_eq!(rollups[0].output_tokens, 7);
    assert_eq!(rollups[0].image_output_tokens, 0);

    let backend = db.conn.get_database_backend();
    for (table, new_column, removed_column) in [
        ("price_rules", "image_output_price", Some("image_price")),
        ("usages", "image_output_tokens", None),
        ("usage_rollups", "image_output_tokens", None),
    ] {
        let columns = db
            .conn
            .query_all_raw(Statement::from_string(
                backend,
                format!("PRAGMA table_info({table})"),
            ))
            .await
            .expect("columns")
            .into_iter()
            .map(|row| row.try_get::<String>("", "name").expect("column name"))
            .collect::<Vec<_>>();
        assert!(columns.iter().any(|column| column == new_column));
        if let Some(removed_column) = removed_column {
            assert!(!columns.iter().any(|column| column == removed_column));
        }
    }
}
