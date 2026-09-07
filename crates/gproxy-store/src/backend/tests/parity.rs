use std::collections::{BTreeMap, BTreeSet};

use super::super::{Executor, Statement};
use super::{libsql_store, native_store, scenario};
use crate::schema::{Dialect, tables};

#[tokio::test]
async fn native_and_libsql_share_schema_and_query_behavior() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    for path in [
        native_dir.path().join("native.db"),
        remote_dir.path().join("remote.db"),
    ] {
        let database = super::super::native::NativeSql::open(path).await.unwrap();
        crate::migration::migrate_to(
            &database,
            Dialect::NativeSqlite,
            crate::schema::SchemaVersion::Initial,
        )
        .await
        .unwrap();
    }
    let (native, native_db) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let (libsql, remote_db) = libsql_store(remote_dir.path().join("remote.db"))
        .await
        .expect("libsql store");

    assert_eq!(schema_shape(native_db.as_ref()).await, expected_shape());
    assert_eq!(index_shape(native_db.as_ref()).await, expected_indexes());
    let versions = crate::schema::SchemaVersion::ALL
        .map(|version| version.number())
        .to_vec();
    assert_eq!(migration_versions(native_db.as_ref()).await, versions);
    assert_eq!(migration_versions(remote_db.as_ref()).await, versions);
    assert_eq!(
        schema_shape(native_db.as_ref()).await,
        schema_shape(remote_db.as_ref()).await
    );
    assert_eq!(
        index_shape(native_db.as_ref()).await,
        index_shape(remote_db.as_ref()).await
    );
    assert_eq!(scenario::run(&native).await, scenario::run(&libsql).await);
}

#[tokio::test]
async fn size_pressure_purges_logs_and_preserves_usage_history() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    let (native, _) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let (libsql, _) = libsql_store(remote_dir.path().join("remote.db"))
        .await
        .expect("libsql store");

    for store in [native, libsql] {
        scenario::run(&store).await;
        let result = store
            .cleanup_observability(None, Some(1))
            .await
            .expect("size-pressure sweep");
        assert!(result.over_size_limit);
        assert_eq!(result.pressure_rows, 5);
        assert_eq!(row_count(&store, "request_logs").await, 0);
        assert_eq!(row_count(&store, "wire_logs").await, 0);
        assert_eq!(row_count(&store, "usage_rows").await, 2);
    }
}

#[tokio::test]
async fn native_and_libsql_batch_failure_rolls_back() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    let (_, native) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let (libsql, _) = libsql_store(remote_dir.path().join("remote.db"))
        .await
        .expect("libsql store");
    assert_batch_rollback(native.as_ref()).await;
    assert_batch_rollback(libsql.backend()).await;
}

#[tokio::test]
#[ignore = "requires empty PostgreSQL and MySQL databases via GPROXY_TEST_POSTGRES_DSN and GPROXY_TEST_MYSQL_DSN"]
async fn postgres_and_mysql_share_schema_queries_and_rollback() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let (native, _) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let expected = scenario::run(&native).await;
    for (config, dialect) in [
        (
            crate::BackendConfig::Postgres {
                dsn: std::env::var("GPROXY_TEST_POSTGRES_DSN").expect("GPROXY_TEST_POSTGRES_DSN"),
            },
            Dialect::Postgres,
        ),
        (
            crate::BackendConfig::Mysql {
                dsn: std::env::var("GPROXY_TEST_MYSQL_DSN").expect("GPROXY_TEST_MYSQL_DSN"),
            },
            Dialect::Mysql,
        ),
    ] {
        let store = crate::Store::open(config).await.expect("SQL store");
        assert_eq!(
            table_names(&store, dialect).await,
            expected_shape().into_keys().collect()
        );
        assert_eq!(scenario::run(&store).await, expected);
        assert_batch_rollback(store.backend()).await;
    }
}

async fn assert_batch_rollback(executor: &dyn Executor) {
    executor
        .execute(Statement::plain(
            "DROP TABLE IF EXISTS gproxy_batch_rollback_test",
        ))
        .await
        .expect("drop rollback table");
    executor
        .execute(Statement::plain("CREATE TABLE gproxy_batch_rollback_test (id BIGINT PRIMARY KEY, value BIGINT NOT NULL)"))
        .await
        .expect("create rollback table");
    executor
        .execute(Statement::plain(
            "INSERT INTO gproxy_batch_rollback_test(id,value) VALUES(1,10)",
        ))
        .await
        .expect("seed rollback table");
    let result = executor
        .batch(vec![
            Statement::plain("UPDATE gproxy_batch_rollback_test SET value=20 WHERE id=1"),
            Statement::plain("INSERT INTO gproxy_missing_table(id) VALUES(1)"),
        ])
        .await;
    assert!(result.is_err());
    let result = executor
        .execute(Statement::plain(
            "SELECT value FROM gproxy_batch_rollback_test WHERE id=1",
        ))
        .await
        .expect("read rollback value");
    assert_eq!(result.rows[0].i64("value").expect("rollback value"), 10);
}

async fn table_names(store: &crate::Store, dialect: Dialect) -> BTreeSet<String> {
    let sql = match dialect {
        Dialect::Postgres => {
            "SELECT table_name AS name FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'"
        }
        Dialect::Mysql => {
            "SELECT table_name AS name FROM information_schema.tables WHERE table_schema=DATABASE() AND table_type='BASE TABLE'"
        }
        Dialect::NativeSqlite | Dialect::Libsql => unreachable!("live SQL dialect"),
    };
    store
        .backend()
        .execute(Statement::plain(sql))
        .await
        .expect("table catalog")
        .rows
        .into_iter()
        .map(|row| row.text("name").expect("table name").to_owned())
        .filter(|name| name != "gproxy_batch_rollback_test")
        .collect()
}

async fn row_count(store: &crate::Store, table: &str) -> i64 {
    store
        .backend()
        .execute(Statement::plain(format!(
            "SELECT COUNT(*) AS count FROM {table}"
        )))
        .await
        .expect("row count")
        .rows[0]
        .i64("count")
        .expect("count")
}

async fn migration_versions(executor: &dyn Executor) -> Vec<i64> {
    executor
        .execute(Statement::plain(
            "SELECT version FROM schema_migrations ORDER BY version",
        ))
        .await
        .expect("migration history")
        .rows
        .iter()
        .map(|row| row.i64("version").expect("migration version"))
        .collect()
}

async fn schema_shape(executor: &dyn Executor) -> BTreeMap<String, BTreeSet<String>> {
    let tables = executor
        .execute(Statement::plain(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        ))
        .await
        .expect("table catalog");
    let mut shape = BTreeMap::new();
    for row in tables.rows {
        let table = row.text("name").expect("table name").to_owned();
        let escaped = table.replace('"', "\"\"");
        let columns = executor
            .execute(Statement::plain(format!(
                "PRAGMA table_info(\"{escaped}\")"
            )))
            .await
            .expect("column catalog")
            .rows
            .into_iter()
            .map(|row| row.text("name").expect("column name").to_owned())
            .collect();
        shape.insert(table, columns);
    }
    shape
}

fn expected_shape() -> BTreeMap<String, BTreeSet<String>> {
    let mut expected: BTreeMap<_, _> = tables()
        .map(|table| {
            (
                table.name.to_owned(),
                table
                    .columns
                    .iter()
                    .map(|column| column.name.to_owned())
                    .collect(),
            )
        })
        .collect();
    expected.insert(
        "schema_migrations".into(),
        ["version".into(), "applied_at".into()]
            .into_iter()
            .collect(),
    );
    expected
}

async fn index_shape(executor: &dyn Executor) -> BTreeMap<String, (String, Vec<String>, bool)> {
    let indexes = executor
        .execute(Statement::plain(
            "SELECT name, tbl_name, sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL ORDER BY name",
        ))
        .await
        .expect("index catalog");
    let mut shape = BTreeMap::new();
    for row in indexes.rows {
        let name = row.text("name").expect("index name").to_owned();
        let table = row.text("tbl_name").expect("index table").to_owned();
        let unique = row
            .text("sql")
            .expect("index SQL")
            .starts_with("CREATE UNIQUE INDEX");
        let escaped = name.replace('"', "\"\"");
        let columns = executor
            .execute(Statement::plain(format!(
                "PRAGMA index_info(\"{escaped}\")"
            )))
            .await
            .expect("index columns")
            .rows
            .into_iter()
            .map(|row| row.text("name").expect("index column").to_owned())
            .collect();
        shape.insert(name, (table, columns, unique));
    }
    shape
}

fn expected_indexes() -> BTreeMap<String, (String, Vec<String>, bool)> {
    tables()
        .flat_map(|table| {
            table.indexes.iter().map(|index| {
                (
                    index.name.to_owned(),
                    (
                        table.name.to_owned(),
                        index
                            .columns
                            .iter()
                            .map(|column| (*column).into())
                            .collect(),
                        index.unique,
                    ),
                )
            })
        })
        .collect()
}

#[tokio::test]
async fn route_ownership_step_purges_orphaned_members_and_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    let database = super::super::native::NativeSql::open(dir.path().join("orphans.db"))
        .await
        .unwrap();
    crate::migration::migrate_to(
        &database,
        Dialect::NativeSqlite,
        crate::schema::SchemaVersion::CredentialBudgets,
    )
    .await
    .unwrap();
    for sql in [
        "INSERT INTO providers (name, channel, settings_json, enabled) VALUES ('p', 'custom', '{}', 1)",
        "INSERT INTO routes (name, max_attempts, enabled) VALUES ('kept', 1, 1)",
        "INSERT INTO route_members (route_id, provider_id, credential_id, upstream_model, tier, priority, weight, enabled) VALUES (1, 1, NULL, 'kept', 0, 0, 100, 1)",
        "INSERT INTO route_members (route_id, provider_id, credential_id, upstream_model, tier, priority, weight, enabled) VALUES (99, 1, NULL, 'orphan', 0, 0, 100, 1)",
        "INSERT INTO exposed_models (name, route_id, enabled) VALUES ('kept-model', 1, 1)",
        "INSERT INTO exposed_models (name, route_id, enabled) VALUES ('orphan-model', 99, 1)",
    ] {
        database.execute(Statement::plain(sql)).await.unwrap();
    }
    crate::migration::migrate(&database, Dialect::NativeSqlite)
        .await
        .unwrap();
    for (table, expected) in [("route_members", "kept"), ("exposed_models", "kept-model")] {
        let column = if table == "route_members" {
            "upstream_model"
        } else {
            "name"
        };
        let rows = database
            .execute(Statement::plain(format!(
                "SELECT {column} AS value FROM {table}"
            )))
            .await
            .unwrap()
            .rows;
        let values: Vec<&str> = rows.iter().map(|row| row.text("value").unwrap()).collect();
        assert_eq!(values, [expected], "{table}");
    }
}

/// Every integer reference column is owned by the table it names, or is
/// listed here as history that deliberately outlives its subject.
#[test]
fn every_reference_column_is_owned_or_history() {
    use crate::schema::{ColumnKind, Ownership, tables};
    const HISTORY: &[(&str, &str)] = &[
        ("usage_rows", "provider_id"),
        ("usage_rows", "credential_id"),
        ("usage_rows", "organization_id"),
        ("usage_rows", "team_id"),
        ("usage_rows", "user_id"),
        ("usage_rows", "user_key_id"),
        ("usage_rollups", "provider_id"),
        ("usage_rollups", "organization_id"),
        ("usage_rollups", "team_id"),
        ("usage_rollups", "user_id"),
        ("credential_quota_cycles", "credential_id"),
        ("wire_logs", "provider_id"),
        ("wire_logs", "credential_id"),
        ("admin_audit_events", "actor_user_id"),
        ("admin_audit_events", "target_id"),
        // A route member picks a provider, never a credential; the column
        // stays in the schema unread because dropping columns is not portable.
        ("route_members", "credential_id"),
    ];
    let owned: Vec<(&str, &str)> = tables()
        .flat_map(|spec| spec.owns.iter())
        .map(|ownership| match *ownership {
            Ownership::Owns { table, column } | Ownership::Detaches { table, column } => {
                (table, column)
            }
            Ownership::Scoped { table, .. } => (table, "subject_id"),
        })
        .collect();
    let mut unowned = Vec::new();
    for spec in tables() {
        for column in spec.columns {
            let reference = column.kind == ColumnKind::Integer
                && column.name != "id"
                && column.name.ends_with("_id");
            if reference
                && !HISTORY.contains(&(spec.name, column.name))
                && !owned.contains(&(spec.name, column.name))
            {
                unowned.push(format!("{}.{}", spec.name, column.name));
            }
        }
    }
    assert!(
        unowned.is_empty(),
        "reference columns nobody owns: {unowned:?}"
    );
    for ownership in tables().flat_map(|spec| spec.owns.iter()) {
        assert!(
            tables().any(|spec| spec.name == ownership.table()),
            "ownership names unknown table {}",
            ownership.table()
        );
    }
}

#[tokio::test]
async fn owned_rows_step_sweeps_every_declared_orphan() {
    let dir = tempfile::tempdir().expect("tempdir");
    let database = super::super::native::NativeSql::open(dir.path().join("owned.db"))
        .await
        .unwrap();
    crate::migration::migrate_to(
        &database,
        Dialect::NativeSqlite,
        crate::schema::SchemaVersion::RouteOwnership,
    )
    .await
    .unwrap();
    for sql in [
        "INSERT INTO price_rules (provider_id, model_pattern, tiers_json, priority, enabled) VALUES (NULL, '*', NULL, 0, 1)",
        "INSERT INTO price_rates (rule_id, metric, unit_size, price, conditions_json, priority) VALUES (1, 'input_tokens', 1, '1', NULL, 0)",
        "INSERT INTO price_rates (rule_id, metric, unit_size, price, conditions_json, priority) VALUES (7, 'input_tokens', 1, '1', NULL, 0)",
        "INSERT INTO organizations (name, enabled) VALUES ('org', 1)",
        "INSERT INTO teams (organization_id, name, enabled) VALUES (1, 'kept', 1)",
        "INSERT INTO users (name, organization_id, team_id, enabled, is_admin) VALUES ('u', 1, 42, 1, 0)",
        "INSERT INTO permissions (subject_kind, subject_id, provider_id, operation_group, allowed) VALUES ('team', 1, NULL, NULL, 1)",
        "INSERT INTO permissions (subject_kind, subject_id, provider_id, operation_group, allowed) VALUES ('team', 9, NULL, NULL, 1)",
        "INSERT INTO quotas (subject_kind, subject_id, quota_total, enabled) VALUES ('user_key', 55, '1', 1)",
        "INSERT INTO quota_windows (quota_id, window_kind, window_start, reset_at, cost_used, active_slot) VALUES (1, 'daily', 0, NULL, '0', 1)",
    ] {
        database.execute(Statement::plain(sql)).await.unwrap();
    }
    crate::migration::migrate(&database, Dialect::NativeSqlite)
        .await
        .unwrap();
    let database = &database;
    let count = |sql: &'static str| async move {
        database
            .execute(Statement::plain(sql))
            .await
            .unwrap()
            .rows
            .len()
    };
    assert_eq!(count("SELECT id FROM price_rates").await, 1);
    assert_eq!(count("SELECT id FROM permissions").await, 1);
    assert_eq!(count("SELECT id FROM quotas").await, 0);
    assert_eq!(count("SELECT id FROM quota_windows").await, 0);
    assert_eq!(count("SELECT id FROM users WHERE team_id IS NULL").await, 1);
}
