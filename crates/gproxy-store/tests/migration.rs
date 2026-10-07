#![cfg(not(target_arch = "wasm32"))]
//! The claim this file exists to hold up: the schema a fresh database gets from
//! the migrator and the schema it gets from the fast path are the same schema,
//! column for column and index for index.
//!
//! Today those two paths share the baseline, so the claim is nearly free. It
//! stops being free the moment a second migration is appended: from then on one
//! path walks the chain and the other reads the registry, and only this test
//! notices when they stop agreeing — which is exactly what happens when somebody
//! changes an entity and forgets to write the migration that carries an existing
//! database to it. `a_guarded_migration_is_a_no_op_where_the_baseline_already_
//! did_it` rehearses that future with a second migration of its own.

use gproxy_seaorm::{
    SchemaProbeExt, SchemaSyncConnectionTrait,
    sea_orm_migration::{
        MigrationName, MigrationStatus, MigrationTrait, MigratorTrait, SchemaManager,
    },
};
use gproxy_store::{MIGRATION_LEDGER, Migrator, SchemaState, Store};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, Statement,
    sea_query::{ColumnDef, Table},
};

async fn connection() -> DatabaseConnection {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    Database::connect(options).await.unwrap()
}

async fn rows(db: &DatabaseConnection, sql: &str) -> Vec<sea_orm::QueryResult> {
    db.query_all_raw(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .unwrap()
}

/// Every table and index the database holds, with the DDL that made it. Ordered
/// by name so two databases built in different orders still compare equal, and
/// including the `sql` text so a differing column type or constraint shows up
/// rather than hiding behind a matching name.
async fn schema(db: &DatabaseConnection) -> Vec<(String, String, String)> {
    rows(
        db,
        "SELECT type, name, COALESCE(sql, '') AS sql FROM sqlite_schema \
         WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
    )
    .await
    .into_iter()
    .map(|row| {
        (
            row.try_get("", "type").unwrap(),
            row.try_get("", "name").unwrap(),
            row.try_get("", "sql").unwrap(),
        )
    })
    .collect()
}

/// The column-by-column half, read from the backend rather than from the DDL
/// text: name, declared type, nullability, default and primary-key position.
async fn columns(db: &DatabaseConnection, table: &str) -> Vec<String> {
    rows(
        db,
        &format!(
            "SELECT name, type, \"notnull\", COALESCE(dflt_value, '') AS dflt, pk \
             FROM pragma_table_info('{table}') ORDER BY name"
        ),
    )
    .await
    .into_iter()
    .map(|row| {
        format!(
            "{}:{}:{}:{}:{}",
            row.try_get::<String>("", "name").unwrap(),
            row.try_get::<String>("", "type").unwrap(),
            row.try_get::<i64>("", "notnull").unwrap(),
            row.try_get::<String>("", "dflt").unwrap(),
            row.try_get::<i64>("", "pk").unwrap(),
        )
    })
    .collect()
}

async fn tables(db: &DatabaseConnection) -> Vec<String> {
    schema(db)
        .await
        .into_iter()
        .filter(|(kind, ..)| kind == "table")
        .map(|(_, name, _)| name)
        .collect()
}

async fn assert_same_schema(left: &DatabaseConnection, right: &DatabaseConnection) {
    let (left_schema, right_schema) = (schema(left).await, schema(right).await);
    assert_eq!(
        left_schema
            .iter()
            .map(|(_, name, _)| name)
            .collect::<Vec<_>>(),
        right_schema
            .iter()
            .map(|(_, name, _)| name)
            .collect::<Vec<_>>(),
        "different objects"
    );
    for ((kind, name, left_sql), (.., right_sql)) in left_schema.iter().zip(&right_schema) {
        assert_eq!(left_sql, right_sql, "{kind} {name} differs");
        if kind == "table" {
            assert_eq!(
                columns(left, name).await,
                columns(right, name).await,
                "{name} columns differ"
            );
        }
    }
    assert!(
        left_schema.iter().any(|(_, name, _)| name == "users"),
        "compared two empty databases"
    );
}

/// The whole claim of the baseline, in one assertion.
#[tokio::test]
async fn the_migrator_and_the_installer_agree_on_a_fresh_database() {
    let installed = connection().await;
    Store::new(installed.clone()).install().await.unwrap();

    let migrated = connection().await;
    migrated.run_migrations::<Migrator>(None).await.unwrap();

    assert_same_schema(&installed, &migrated).await;
    // Both ways round, the ledger says the same thing.
    assert_eq!(
        Store::new(installed).migration_report().await.unwrap(),
        Store::new(migrated).migration_report().await.unwrap()
    );
}

#[tokio::test]
async fn running_the_migrator_twice_is_a_no_op() {
    let store = Store::new(connection().await);
    assert_eq!(store.schema_state().await.unwrap(), SchemaState::Empty);

    let first = store.migrate().await.unwrap();
    assert!(first.installed);
    assert_eq!(
        first.ledger,
        [
            "m20260921_000001_baseline",
            "m20260926_000001_credential_cycles",
            "m20260930_000001_allowed_headers",
            "m20261005_000001_capture_storage",
            "m20261007_000001_rewrite_conditions",
            "m20261008_000001_usage_timing"
        ]
    );
    let after_first = schema(store.connection()).await;

    let second = store.migrate().await.unwrap();
    assert!(!second.installed);
    assert!(second.applied.is_empty(), "{second:?}");
    assert_eq!(second.ledger, first.ledger);
    assert_eq!(schema(store.connection()).await, after_first);
    assert_eq!(store.schema_state().await.unwrap(), SchemaState::Managed);
}

#[tokio::test]
async fn the_report_says_what_was_applied() {
    let store = Store::new(connection().await);
    let names: Vec<String> = Migrator::migrations()
        .iter()
        .map(|migration| migration.name().to_owned())
        .collect();

    // Pending before, and asking did not create the ledger it was asking about.
    assert_eq!(
        store.migration_report().await.unwrap(),
        names
            .iter()
            .map(|name| (name.clone(), MigrationStatus::Pending))
            .collect::<Vec<_>>()
    );
    assert_eq!(store.schema_state().await.unwrap(), SchemaState::Empty);

    store.migrate().await.unwrap();
    assert_eq!(
        store.migration_report().await.unwrap(),
        names
            .iter()
            .map(|name| (name.clone(), MigrationStatus::Applied))
            .collect::<Vec<_>>()
    );
    assert!(
        tables(store.connection())
            .await
            .contains(&MIGRATION_LEDGER.into())
    );
}

/// The refusal, and — the point of it — that the refusal happened before
/// anything was written.
#[tokio::test]
async fn a_database_this_build_did_not_create_is_refused_untouched() {
    let store = Store::new(connection().await);
    for statement in [
        "CREATE TABLE schema_migrations (version TEXT NOT NULL PRIMARY KEY)",
        "CREATE TABLE users (id INTEGER NOT NULL PRIMARY KEY, name TEXT)",
        "INSERT INTO users (id, name) VALUES (1, 'kept')",
    ] {
        store
            .connection()
            .execute_unprepared(statement)
            .await
            .unwrap();
    }
    let before = schema(store.connection()).await;

    let state = store.schema_state().await.unwrap();
    assert_eq!(
        state.foreign_tables().unwrap(),
        ["schema_migrations", "users"]
    );

    let error = store.migrate().await.unwrap_err().to_string();
    assert!(error.contains(MIGRATION_LEDGER), "{error}");
    assert!(error.contains("schema_migrations"), "{error}");

    // Not one statement ran: `users.id` is still the integer it was, which is
    // the very column the original failure warned about and then tried to work
    // around.
    assert_eq!(schema(store.connection()).await, before);
    assert_eq!(
        columns(store.connection(), "users").await,
        ["id:INTEGER:1::1", "name:TEXT:0::0"]
    );
}

/// A second migration, written the way the convention says to write one, and
/// the two databases it has to work on: the fresh one where the baseline has
/// already done its job, and the existing one where it has not.
struct AddedColumn;

impl MigrationName for AddedColumn {
    fn name(&self) -> &str {
        "m20260922_000001_add_a_column"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddedColumn {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.probe_column("users", "note").await? {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table("users")
                    .add_column(ColumnDef::new("note").text().null())
                    .to_owned(),
            )
            .await
    }
}

struct Later;

impl MigratorTrait for Later {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        let mut migrations = Migrator::migrations();
        migrations.push(Box::new(AddedColumn));
        migrations
    }
}

#[tokio::test]
async fn a_guarded_migration_is_a_no_op_where_the_baseline_already_did_it() {
    // The fresh database: the baseline creates the registry, and since the
    // registry is read live it already has whatever the entities say. Stand in
    // for that by letting the baseline run and then adding the column by hand,
    // which is what a registry containing it would have produced.
    let fresh = connection().await;
    fresh.run_migrations::<Migrator>(None).await.unwrap();
    fresh
        .execute_unprepared("ALTER TABLE users ADD COLUMN note text")
        .await
        .unwrap();
    fresh.run_migrations::<Later>(None).await.unwrap();

    // The existing database: baseline only, column genuinely missing.
    let existing = connection().await;
    existing.run_migrations::<Migrator>(None).await.unwrap();
    existing.run_migrations::<Later>(None).await.unwrap();

    assert_eq!(
        columns(&fresh, "users").await,
        columns(&existing, "users").await
    );
    assert!(
        columns(&existing, "users")
            .await
            .iter()
            .any(|column| column.starts_with("note:")),
        "the migration did not run where it had work to do"
    );
    for db in [&fresh, &existing] {
        let before = columns(db, "users").await;
        let error = Store::new(db.clone()).migrate().await.unwrap_err();
        assert!(error.to_string().contains("m20260922_000001_add_a_column"));
        assert_eq!(columns(db, "users").await, before);
        assert_eq!(
            db.migration_report::<Later>()
                .await
                .unwrap()
                .into_iter()
                .filter(|(_, status)| *status == MigrationStatus::Applied)
                .count(),
            Migrator::migrations().len() + 1
        );
    }
}

/// A database from before credential cycles: no cycle table, no link columns
/// or time index on the observation log, no retention setting. The migration
/// carries it to exactly what a fresh install has.
#[tokio::test]
async fn the_cycle_migration_brings_an_older_database_to_the_fresh_schema() {
    let fresh = connection().await;
    Store::new(fresh.clone()).install().await.unwrap();

    let older = connection().await;
    Store::new(older.clone()).install().await.unwrap();
    for sql in [
        "DROP TABLE credential_cycles",
        "DROP INDEX \"idx-credential_quota_cycles-by_credential\"",
        "DROP INDEX \"idx-credential_quota_cycles-observed_at_ms\"",
        "ALTER TABLE credential_quota_cycles DROP COLUMN credential_cycle_id",
        "ALTER TABLE credential_quota_cycles DROP COLUMN cycle_cost_usd",
        "ALTER TABLE settings DROP COLUMN quota_observation_retention_days",
        "DELETE FROM seaql_migrations WHERE version = 'm20260926_000001_credential_cycles'",
    ] {
        older.execute_unprepared(sql).await.unwrap();
    }
    let report = Store::new(older.clone()).migrate().await.unwrap();
    assert_eq!(report.applied, ["m20260926_000001_credential_cycles"]);
    for table in ["credential_cycles", "credential_quota_cycles", "settings"] {
        assert_eq!(
            columns(&older, table).await,
            columns(&fresh, table).await,
            "{table}"
        );
    }
    let indexes = |db| async move {
        schema(db)
            .await
            .into_iter()
            .filter(|(kind, name, _)| {
                kind == "index"
                    && (name.contains("credential_cycles")
                        || name.contains("credential_quota_cycles"))
            })
            .map(|(_, name, _)| name)
            .collect::<Vec<_>>()
    };
    assert_eq!(indexes(&older).await, indexes(&fresh).await);
}

#[tokio::test]
async fn capture_upgrade_backfills_headers_and_preserves_legacy_identity_bodies() {
    use gproxy_store::entity::usage::{downstream_record, header_set};
    use sea_orm::{EntityTrait, Set};
    let db = connection().await;
    let store = Store::new(db.clone());
    store.install().await.unwrap();
    // Rebuild the empty capture tables without this migration's columns/FKs,
    // retaining the old entity columns, primary keys and owner/session FKs.
    use gproxy_store::entity::usage::{downstream_event, upstream_event, upstream_record};
    let schema = sea_orm::Schema::new(DbBackend::Sqlite);
    let definitions = [
        (
            "upstream_records",
            schema.create_table_from_entity(upstream_record::Entity),
        ),
        (
            "downstream_records",
            schema.create_table_from_entity(downstream_record::Entity),
        ),
        (
            "upstream_events",
            schema.create_table_from_entity(upstream_event::Entity),
        ),
        (
            "downstream_events",
            schema.create_table_from_entity(downstream_event::Entity),
        ),
    ];
    for table in [
        "capture_body_blobs",
        "capture_bodies",
        "capture_blobs",
        "upstream_events",
        "downstream_events",
        "upstream_records",
        "downstream_records",
        "header_sets",
    ] {
        db.execute_unprepared(&format!("DROP TABLE {table}"))
            .await
            .unwrap();
    }
    let added = [
        "request_headers_hash",
        "response_headers_hash",
        "request_body_id",
        "request_body_encoding",
        "response_body_encoding",
        "encoding",
        "chunk_offsets",
        "body_id",
    ];
    for (name, definition) in definitions {
        let mut legacy = Table::create();
        legacy.table(name);
        for column in definition.get_columns() {
            if !added.contains(&column.get_column_name().as_str()) {
                legacy.col(column.clone());
            }
        }
        for index in definition.get_indexes() {
            legacy.index(&mut index.clone());
        }
        for key in definition.get_foreign_key_create_stmts() {
            if !key
                .get_foreign_key()
                .get_columns()
                .iter()
                .any(|column| added.contains(&column.as_str()))
            {
                legacy.foreign_key(&mut key.clone());
            }
        }
        db.execute_raw(DbBackend::Sqlite.build(&legacy))
            .await
            .unwrap();
    }
    for name in ["capture_payload_retention_days", "capture_payload_max_mb"] {
        db.execute_unprepared(&format!("ALTER TABLE settings DROP COLUMN {name}"))
            .await
            .unwrap();
    }
    db.execute_unprepared(
        "DELETE FROM seaql_migrations WHERE version = 'm20261005_000001_capture_storage'",
    )
    .await
    .unwrap();
    // ActiveModels omit the not-yet-existing defaulted columns on insert.
    store
        .downstream_records()
        .insert_many(
            ["legacy-a", "legacy-b"]
                .into_iter()
                .map(|id| downstream_record::ActiveModel {
                    id: Set(id.into()),
                    kind: Set(downstream_record::CaptureKind::Http),
                    started_at_ms: Set(1),
                    request_headers: Set(Some(serde_json::json!([["X-Test", "same"]]))),
                    request_body: Set(Some(b"legacy request".to_vec())),
                    response_body: Set(Some(b"legacy response".to_vec())),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
    let report = store.migrate().await.unwrap();
    assert_eq!(report.applied, ["m20261005_000001_capture_storage"]);
    assert_eq!(
        store
            .header_sets()
            .query(header_set::Entity::find())
            .await
            .unwrap()
            .len(),
        1
    );
    for row in store
        .downstream_records()
        .query(downstream_record::Entity::find())
        .await
        .unwrap()
    {
        assert!(row.request_headers.is_none());
        assert!(row.request_headers_hash.is_some());
        assert_eq!(row.request_body_encoding, "identity");
        let hydrated = store.hydrate_capture(row.into()).await.unwrap();
        assert_eq!(
            hydrated.request_headers,
            Some(serde_json::json!([["x-test", "same"]]))
        );
        assert_eq!(
            hydrated.request_body.as_deref(),
            Some(b"legacy request".as_slice())
        );
        assert_eq!(
            hydrated.response_body.as_deref(),
            Some(b"legacy response".as_slice())
        );
    }
    assert!(store.migrate().await.unwrap().applied.is_empty());
}

#[tokio::test]
async fn usage_timing_upgrade_matches_fresh_columns() {
    let fresh = connection().await;
    Store::new(fresh.clone()).install().await.unwrap();
    let older = connection().await;
    Store::new(older.clone()).install().await.unwrap();
    for sql in [
        "ALTER TABLE usage_records DROP COLUMN duration_ms",
        "ALTER TABLE usage_records DROP COLUMN ttft_ms",
        "DELETE FROM seaql_migrations WHERE version = 'm20261008_000001_usage_timing'",
    ] {
        older.execute_unprepared(sql).await.unwrap();
    }
    let report = Store::new(older.clone()).migrate().await.unwrap();
    assert_eq!(report.applied, ["m20261008_000001_usage_timing"]);
    assert_eq!(
        columns(&older, "usage_records").await,
        columns(&fresh, "usage_records").await
    );
}
