//! Schema creation on connect. Derives `CREATE TABLE IF NOT EXISTS` from the
//! SeaORM entities for whatever dialect the connection uses (single source of
//! truth = the entity definitions; no separate migration crate yet).

use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Schema, Statement};

use crate::store::persistence::migrations::{
    CREATE_MIGRATIONS_TABLE, SELECT_MAX_VERSION, latest_version, pending,
};

use super::entities::authz::{quota, rate_limit, route_permission};
use super::entities::identity::{org, team, user, user_key};
use super::entities::logs::{audit_log, downstream_request, upstream_request};
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

    create_composite_unique_indexes(conn).await?;

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
async fn create_composite_unique_indexes(conn: &DatabaseConnection) -> anyhow::Result<()> {
    let mysql = matches!(conn.get_database_backend(), sea_orm::DatabaseBackend::MySql);
    let defs = [
        ("uq_teams_org_name", "teams", "org_id, name"),
        (
            "uq_routing_rules_dims",
            "routing_rules",
            "provider_id, operation, kind",
        ),
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

    // Writes go through `execute_unprepared` (raw SQL, dialect-portable). The
    // version read uses `query_one_raw`, which takes a `Statement` by value.
    conn.execute_unprepared(CREATE_MIGRATIONS_TABLE).await?;

    let current = conn
        .query_one_raw(Statement::from_string(backend, SELECT_MAX_VERSION))
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
        for sql in m.sql {
            conn.execute_unprepared(sql).await?;
        }
        record_version(conn, m.version).await?;
    }
    Ok(())
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
