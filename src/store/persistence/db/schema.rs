//! Schema creation on connect. Derives `CREATE TABLE IF NOT EXISTS` from the
//! SeaORM entities for whatever dialect the connection uses (single source of
//! truth = the entity definitions; no separate migration crate yet).

use std::collections::HashSet;

use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Schema, Statement};

use crate::store::persistence::migrations::{
    CREATE_MIGRATIONS_TABLE, MigrationDialect, latest_version, pending, select_max_version_sql,
};

use super::entities::authz::{quota, rate_limit, route_permission};
use super::entities::identity::{org, team, user, user_key};
use super::entities::logs::{audit_log, downstream_request, upstream_request};
use super::entities::pricing::price_rule;
use super::entities::provider::{credential, credential_status, provider, provider_model};
use super::entities::routing::{alias, route, route_member};
use super::entities::settings::instance_setting;
use super::entities::tokenize::tokenizer_vocab;
use super::entities::transform::{provider_rule_set, routing_rule, rule, rule_set};
use super::entities::usage::{usage, usage_rollup};

pub(super) async fn create_all(conn: &DatabaseConnection) -> anyhow::Result<()> {
    let backend = conn.get_database_backend();
    let schema = Schema::new(backend);

    create_table(conn, &schema, provider::Entity).await?;
    create_table(conn, &schema, credential::Entity).await?;
    create_table(conn, &schema, credential_status::Entity).await?;
    create_table(conn, &schema, provider_model::Entity).await?;
    create_table(conn, &schema, price_rule::Entity).await?;
    create_table(conn, &schema, route::Entity).await?;
    create_table(conn, &schema, route_member::Entity).await?;
    create_table(conn, &schema, alias::Entity).await?;

    // §8-B2 rules
    create_table(conn, &schema, routing_rule::Entity).await?;
    create_table(conn, &schema, rule_set::Entity).await?;
    create_table(conn, &schema, rule::Entity).await?;
    create_table(conn, &schema, provider_rule_set::Entity).await?;

    // §8-C identity
    create_table(conn, &schema, org::Entity).await?;
    create_table(conn, &schema, team::Entity).await?;
    create_table(conn, &schema, user::Entity).await?;
    create_table(conn, &schema, user_key::Entity).await?;
    create_table(conn, &schema, route_permission::Entity).await?;
    create_table(conn, &schema, rate_limit::Entity).await?;
    create_table(conn, &schema, quota::Entity).await?;

    // §8-D usage
    create_table(conn, &schema, usage::Entity).await?;
    create_table(conn, &schema, usage_rollup::Entity).await?;
    create_rollup_unique_index(conn).await?;
    create_table(conn, &schema, downstream_request::Entity).await?;
    create_table(conn, &schema, upstream_request::Entity).await?;
    create_table(conn, &schema, audit_log::Entity).await?;

    // §8-E settings
    create_table(conn, &schema, instance_setting::Entity).await?;

    // §6.3 tokenizer vocabs
    create_table(conn, &schema, tokenizer_vocab::Entity).await?;

    Ok(())
}

async fn create_table<E: EntityTrait>(
    conn: &DatabaseConnection,
    schema: &Schema,
    entity: E,
) -> anyhow::Result<()> {
    let mut stmt = schema.create_table_from_entity(entity);
    stmt.if_not_exists();
    conn.execute(&stmt).await?;
    Ok(())
}

/// One `usage_rollups` row per dimension bucket: two instances racing the
/// first insert for a bucket must collide here (the loser retries into the
/// accumulate path). COALESCE folds the nullable dimensions, which unique
/// indexes otherwise treat as distinct. Raw SQL because the entity derive
/// can't express a multi-column expression index; MySQL needs each expression
/// parenthesized and has no `IF NOT EXISTS` for indexes, so its duplicate-name
/// error is treated as already-created.
async fn create_rollup_unique_index(conn: &DatabaseConnection) -> anyhow::Result<()> {
    let mysql = matches!(conn.get_database_backend(), sea_orm::DatabaseBackend::MySql);
    let sql = if mysql {
        "CREATE UNIQUE INDEX uq_usage_rollups_dims ON usage_rollups (\
         granularity, bucket_start, \
         (COALESCE(provider_id, 0)), (COALESCE(org_id, 0)), \
         (COALESCE(team_id, 0)), (COALESCE(user_id, 0)), \
         (COALESCE(route_name, '')), (COALESCE(model, '')))"
    } else {
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_usage_rollups_dims ON usage_rollups (\
         granularity, bucket_start, \
         COALESCE(provider_id, 0), COALESCE(org_id, 0), \
         COALESCE(team_id, 0), COALESCE(user_id, 0), \
         COALESCE(route_name, ''), COALESCE(model, ''))"
    };
    match conn.execute_unprepared(sql).await {
        Ok(_) => Ok(()),
        // MySQL 1061 = duplicate key name: the index already exists.
        Err(e) if mysql && e.to_string().contains("1061") => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Composite-unique indexes for the multi-column unique keys (§8-A/B/C). The
/// SeaORM `#[sea_orm(unique)]` derive only covers single columns, so these are
/// raw SQL — making the DB the source of truth for these keys (the app-level
/// pre-checks alone race under concurrency / multi-instance and would otherwise
/// admit duplicate rows). Mirrors `create_rollup_unique_index`'s dialect
/// handling: MySQL has no `IF NOT EXISTS` for indexes, so a duplicate-name
/// error (1061) means the index already exists. Columns are all NOT NULL, so no
/// COALESCE folding is needed.
pub(super) async fn create_composite_unique_indexes(
    conn: &DatabaseConnection,
) -> anyhow::Result<()> {
    let mysql = matches!(conn.get_database_backend(), sea_orm::DatabaseBackend::MySql);
    let defs = [
        ("uq_teams_org_name", "teams", "org_id, name"),
        (
            "uq_routing_rules_dims",
            "routing_rules",
            "provider_id, operation, kind",
        ),
        ("uq_aliases_provider_alias", "aliases", "provider, alias"),
        ("uq_quotas_scope", "quotas", "scope, scope_id"),
    ];
    for (name, table, cols) in defs {
        let sql = if mysql {
            format!("CREATE UNIQUE INDEX {name} ON {table} ({cols})")
        } else {
            format!("CREATE UNIQUE INDEX IF NOT EXISTS {name} ON {table} ({cols})")
        };
        match conn.execute_unprepared(&sql).await {
            Ok(_) => {}
            // MySQL 1061 = duplicate key name: the index already exists.
            Err(e) if mysql && e.to_string().contains("1061") => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Stamp an unstamped DB at the latest version, then apply pending migrations.
///
/// Assumes [`create_all`] has already run, so an unstamped DB holds the
/// *current* schema with every listed migration already reflected in it. We
/// therefore stamp the "no `schema_migrations` row" case at
/// [`latest_version`] WITHOUT running any DDL (replaying e.g. an `ADD COLUMN`
/// would fail against the fresh tables), then apply `version >` migrations in
/// order. DBs created by builds older than this framework are not upgradable
/// in place (see the `migrations` module docs).
pub(super) async fn run_migrations(conn: &DatabaseConnection) -> anyhow::Result<()> {
    let backend = conn.get_database_backend();
    let dialect = match backend {
        sea_orm::DatabaseBackend::Sqlite => MigrationDialect::Sqlite,
        sea_orm::DatabaseBackend::Postgres => MigrationDialect::Postgres,
        sea_orm::DatabaseBackend::MySql => MigrationDialect::MySql,
        _ => anyhow::bail!("unsupported database backend for migrations"),
    };

    // Writes go through `execute_unprepared` (raw SQL, dialect-portable). The
    // version read uses `query_one_raw`, which takes a `Statement` by value.
    conn.execute_unprepared(CREATE_MIGRATIONS_TABLE).await?;

    let current = conn
        .query_one_raw(Statement::from_string(
            backend,
            select_max_version_sql(dialect),
        ))
        .await?
        .map(|row| row.try_get::<i64>("", "v"))
        .transpose()?
        .unwrap_or(0);

    // Empty table → stamp the current schema the create routine just ensured.
    let current = if current == 0 {
        let latest = latest_version();
        record_version(conn, latest).await?;
        latest
    } else {
        current
    };

    for m in pending(current) {
        for sql in m.sql_for(dialect) {
            conn.execute_unprepared(sql).await?;
        }
        record_version(conn, m.version).await?;
    }
    repair_price_rules_schema(conn, dialect).await?;
    Ok(())
}

async fn repair_price_rules_schema(
    conn: &DatabaseConnection,
    dialect: MigrationDialect,
) -> anyhow::Result<()> {
    let cols = table_columns(conn, dialect, "price_rules").await?;
    if cols.is_empty() {
        return Ok(());
    }

    let had_rates_json = cols.contains("rates_json");
    let had_cache_write_price = cols.contains("cache_write_price");
    let had_operation = cols.contains("operation");
    let had_kind = cols.contains("kind");
    let had_priority = cols.contains("priority");
    let mut changed = false;

    for col in [
        "input_price",
        "output_price",
        "cache_read_price",
        "cache_creation_5m_price",
        "cache_creation_1h_price",
        "image_price",
    ] {
        if !cols.contains(col) {
            conn.execute_unprepared(&add_price_column_sql(dialect, col))
                .await?;
            changed = true;
        }
    }

    if changed && had_rates_json {
        let sql = backfill_price_rules_from_rates_json_sql(dialect);
        if let Err(err) = conn.execute_unprepared(sql).await {
            tracing::warn!(error = %err, "price_rules rates_json backfill skipped");
        }
    }

    if changed && had_cache_write_price {
        let sql = "UPDATE price_rules \
                   SET cache_creation_5m_price = cache_write_price, \
                       cache_creation_1h_price = cache_write_price \
                   WHERE cache_write_price IS NOT NULL";
        if let Err(err) = conn.execute_unprepared(sql).await {
            tracing::warn!(error = %err, "price_rules cache_write_price backfill skipped");
        }
    }

    if had_rates_json || had_cache_write_price || had_operation || had_kind || had_priority {
        drop_price_rules_legacy_columns(
            conn,
            dialect,
            &[
                (had_rates_json, "rates_json"),
                (had_cache_write_price, "cache_write_price"),
                (had_operation, "operation"),
                (had_kind, "kind"),
                (had_priority, "priority"),
            ],
        )
        .await?;
    }

    Ok(())
}

async fn drop_price_rules_legacy_columns(
    conn: &DatabaseConnection,
    dialect: MigrationDialect,
    columns: &[(bool, &str)],
) -> anyhow::Result<()> {
    match dialect {
        MigrationDialect::Sqlite => rebuild_sqlite_price_rules_table(conn).await,
        MigrationDialect::Postgres | MigrationDialect::MySql => {
            for col in columns {
                if col.0 {
                    conn.execute_unprepared(&format!(
                        "ALTER TABLE price_rules DROP COLUMN {}",
                        col.1
                    ))
                    .await?;
                }
            }
            Ok(())
        }
    }
}

async fn rebuild_sqlite_price_rules_table(conn: &DatabaseConnection) -> anyhow::Result<()> {
    for sql in [
        "DROP TABLE IF EXISTS price_rules_repaired",
        "CREATE TABLE price_rules_repaired (\
            id INTEGER PRIMARY KEY, \
            provider_id INTEGER, \
            match_type TEXT NOT NULL, \
            model_match TEXT NOT NULL, \
            input_price TEXT NOT NULL, \
            output_price TEXT NOT NULL, \
            cache_read_price TEXT NOT NULL, \
            cache_creation_5m_price TEXT NOT NULL, \
            cache_creation_1h_price TEXT NOT NULL, \
            image_price TEXT NOT NULL, \
            enabled INTEGER NOT NULL, \
            created_at INTEGER NOT NULL, \
            updated_at INTEGER NOT NULL)",
        "INSERT INTO price_rules_repaired \
            (id, provider_id, match_type, model_match, \
             input_price, output_price, cache_read_price, cache_creation_5m_price, \
             cache_creation_1h_price, image_price, enabled, created_at, updated_at) \
         SELECT \
            id, provider_id, match_type, model_match, \
            input_price, output_price, cache_read_price, cache_creation_5m_price, \
            cache_creation_1h_price, image_price, enabled, created_at, updated_at \
         FROM price_rules",
        "DROP TABLE price_rules",
        "ALTER TABLE price_rules_repaired RENAME TO price_rules",
    ] {
        conn.execute_unprepared(sql).await?;
    }
    Ok(())
}

async fn table_columns(
    conn: &DatabaseConnection,
    dialect: MigrationDialect,
    table: &str,
) -> anyhow::Result<HashSet<String>> {
    let backend = conn.get_database_backend();
    let sql = match dialect {
        MigrationDialect::Sqlite => format!("PRAGMA table_info({table})"),
        MigrationDialect::Postgres => format!(
            "SELECT column_name AS name FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = '{table}'"
        ),
        MigrationDialect::MySql => format!(
            "SELECT COLUMN_NAME AS name FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{table}'"
        ),
    };

    Ok(conn
        .query_all_raw(Statement::from_string(backend, sql))
        .await?
        .into_iter()
        .map(|row| row.try_get::<String>("", "name"))
        .collect::<Result<HashSet<_>, _>>()?)
}

fn add_price_column_sql(dialect: MigrationDialect, col: &str) -> String {
    match dialect {
        MigrationDialect::MySql => {
            format!("ALTER TABLE price_rules ADD COLUMN {col} VARCHAR(64) NOT NULL DEFAULT '0'")
        }
        MigrationDialect::Sqlite | MigrationDialect::Postgres => {
            format!("ALTER TABLE price_rules ADD COLUMN {col} TEXT NOT NULL DEFAULT '0'")
        }
    }
}

fn backfill_price_rules_from_rates_json_sql(dialect: MigrationDialect) -> &'static str {
    match dialect {
        MigrationDialect::Sqlite => {
            "UPDATE price_rules SET \
                input_price = COALESCE(CAST(json_extract(rates_json, '$.input_tokens') AS TEXT), CAST(json_extract(rates_json, '$.input') AS TEXT), input_price), \
                output_price = COALESCE(CAST(json_extract(rates_json, '$.output_tokens') AS TEXT), CAST(json_extract(rates_json, '$.output') AS TEXT), output_price), \
                cache_read_price = COALESCE(CAST(json_extract(rates_json, '$.cache_read_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_read') AS TEXT), cache_read_price), \
                cache_creation_5m_price = COALESCE(CAST(json_extract(rates_json, '$.cache_write_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_creation') AS TEXT), cache_creation_5m_price), \
                cache_creation_1h_price = COALESCE(CAST(json_extract(rates_json, '$.cache_write_tokens') AS TEXT), CAST(json_extract(rates_json, '$.cache_creation') AS TEXT), cache_creation_1h_price), \
                image_price = CASE \
                    WHEN json_type(rates_json, '$.image_count') IN ('integer', 'real', 'text') THEN CAST(json_extract(rates_json, '$.image_count') AS TEXT) \
                    WHEN json_type(rates_json, '$.image') IN ('integer', 'real', 'text') THEN CAST(json_extract(rates_json, '$.image') AS TEXT) \
                    ELSE image_price \
                END \
             WHERE rates_json IS NOT NULL AND rates_json <> '' AND json_valid(rates_json)"
        }
        MigrationDialect::Postgres => {
            "UPDATE price_rules SET \
                input_price = COALESCE(rates_json::jsonb->>'input_tokens', rates_json::jsonb->>'input', input_price), \
                output_price = COALESCE(rates_json::jsonb->>'output_tokens', rates_json::jsonb->>'output', output_price), \
                cache_read_price = COALESCE(rates_json::jsonb->>'cache_read_tokens', rates_json::jsonb->>'cache_read', cache_read_price), \
                cache_creation_5m_price = COALESCE(rates_json::jsonb->>'cache_write_tokens', rates_json::jsonb->>'cache_creation', cache_creation_5m_price), \
                cache_creation_1h_price = COALESCE(rates_json::jsonb->>'cache_write_tokens', rates_json::jsonb->>'cache_creation', cache_creation_1h_price), \
                image_price = CASE \
                    WHEN jsonb_typeof(rates_json::jsonb->'image_count') IN ('string', 'number') THEN rates_json::jsonb->>'image_count' \
                    WHEN jsonb_typeof(rates_json::jsonb->'image') IN ('string', 'number') THEN rates_json::jsonb->>'image' \
                    ELSE image_price \
                END \
             WHERE rates_json IS NOT NULL AND rates_json <> ''"
        }
        MigrationDialect::MySql => {
            "UPDATE price_rules SET \
                input_price = COALESCE(JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.input_tokens')), JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.input')), input_price), \
                output_price = COALESCE(JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.output_tokens')), JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.output')), output_price), \
                cache_read_price = COALESCE(JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.cache_read_tokens')), JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.cache_read')), cache_read_price), \
                cache_creation_5m_price = COALESCE(JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.cache_write_tokens')), JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.cache_creation')), cache_creation_5m_price), \
                cache_creation_1h_price = COALESCE(JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.cache_write_tokens')), JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.cache_creation')), cache_creation_1h_price), \
                image_price = CASE \
                    WHEN JSON_TYPE(JSON_EXTRACT(rates_json, '$.image_count')) IN ('INTEGER', 'DOUBLE', 'DECIMAL', 'STRING') THEN JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.image_count')) \
                    WHEN JSON_TYPE(JSON_EXTRACT(rates_json, '$.image')) IN ('INTEGER', 'DOUBLE', 'DECIMAL', 'STRING') THEN JSON_UNQUOTE(JSON_EXTRACT(rates_json, '$.image')) \
                    ELSE image_price \
                END \
             WHERE rates_json IS NOT NULL AND rates_json <> '' AND JSON_VALID(rates_json)"
        }
    }
}

async fn record_version(conn: &DatabaseConnection, version: i64) -> anyhow::Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    conn.execute_unprepared(&format!(
        "INSERT INTO schema_migrations (version, applied_at) VALUES ({version}, {now})"
    ))
    .await?;
    Ok(())
}
