//! Schema creation on connect. Hand-written `CREATE TABLE IF NOT EXISTS` for
//! the 26 tables defined by the SeaORM entities (`db/entities/`), in SQLite
//! dialect. Ids are `INTEGER PRIMARY KEY` (autoincrement-by-default rowid);
//! bools/timestamps are INTEGER; strings/decimals/JSON are TEXT; blobs BLOB.

use crate::store::libsql::LibsqlClient;
use crate::store::persistence::libsql::row::col_i64;
use crate::store::persistence::migrations::{
    CREATE_MIGRATIONS_TABLE, SELECT_MAX_VERSION, latest_version, pending,
};

/// All `CREATE TABLE IF NOT EXISTS` statements, mirroring `db/schema.rs`.
const TABLES: &[&str] = &[
    // ── providers ──
    "CREATE TABLE IF NOT EXISTS providers (\
        id INTEGER PRIMARY KEY, \
        name TEXT NOT NULL UNIQUE, \
        channel TEXT NOT NULL, \
        label TEXT, \
        settings_json TEXT NOT NULL, \
        credential_strategy TEXT NOT NULL, \
        proxy_url TEXT, \
        tls_fingerprint TEXT, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS credentials (\
        id INTEGER PRIMARY KEY, \
        provider_id INTEGER NOT NULL, \
        name TEXT, \
        kind TEXT NOT NULL, \
        secret_json TEXT NOT NULL, \
        weight INTEGER NOT NULL, \
        rpm_limit INTEGER, \
        tpm_limit INTEGER, \
        proxy_url TEXT, \
        tls_fingerprint TEXT, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS credential_statuses (\
        id INTEGER PRIMARY KEY, \
        credential_id INTEGER NOT NULL, \
        channel TEXT NOT NULL, \
        health_kind TEXT NOT NULL, \
        health_json TEXT, \
        checked_at INTEGER, \
        last_error TEXT, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL, \
        UNIQUE(credential_id, channel))",
    "CREATE TABLE IF NOT EXISTS provider_models (\
        id INTEGER PRIMARY KEY, \
        provider_id INTEGER NOT NULL, \
        model_id TEXT NOT NULL, \
        display_name TEXT, \
        pricing_json TEXT, \
        variants_json TEXT, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    // ── routing ──
    "CREATE TABLE IF NOT EXISTS routes (\
        id INTEGER PRIMARY KEY, \
        name TEXT NOT NULL UNIQUE, \
        strategy TEXT NOT NULL, \
        enabled INTEGER NOT NULL, \
        description TEXT, \
        settings_json TEXT, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS route_members (\
        id INTEGER PRIMARY KEY, \
        route_id INTEGER NOT NULL, \
        provider_id INTEGER NOT NULL, \
        upstream_model_id TEXT NOT NULL, \
        weight INTEGER NOT NULL, \
        tier INTEGER NOT NULL, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS aliases (\
        id INTEGER PRIMARY KEY, \
        alias TEXT NOT NULL UNIQUE, \
        route_id INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    // ── transform / rules ──
    "CREATE TABLE IF NOT EXISTS routing_rules (\
        id INTEGER PRIMARY KEY, \
        provider_id INTEGER NOT NULL, \
        operation TEXT NOT NULL, \
        kind TEXT NOT NULL, \
        implementation TEXT NOT NULL, \
        dest_operation TEXT, \
        dest_kind TEXT, \
        sort_order INTEGER NOT NULL, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL, \
        UNIQUE(provider_id, operation, kind))",
    "CREATE TABLE IF NOT EXISTS rule_sets (\
        id INTEGER PRIMARY KEY, \
        name TEXT NOT NULL UNIQUE, \
        enabled INTEGER NOT NULL, \
        description TEXT, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS rules (\
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
    "CREATE TABLE IF NOT EXISTS provider_rule_sets (\
        id INTEGER PRIMARY KEY, \
        provider_id INTEGER NOT NULL, \
        rule_set_id INTEGER NOT NULL, \
        sort_order INTEGER NOT NULL, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    // ── identity ──
    "CREATE TABLE IF NOT EXISTS orgs (\
        id INTEGER PRIMARY KEY, \
        name TEXT NOT NULL UNIQUE, \
        enabled INTEGER NOT NULL, \
        description TEXT, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS teams (\
        id INTEGER PRIMARY KEY, \
        org_id INTEGER NOT NULL, \
        name TEXT NOT NULL, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL, \
        UNIQUE(org_id, name))",
    "CREATE TABLE IF NOT EXISTS users (\
        id INTEGER PRIMARY KEY, \
        name TEXT NOT NULL UNIQUE, \
        org_id INTEGER NOT NULL, \
        team_id INTEGER, \
        password TEXT, \
        enabled INTEGER NOT NULL, \
        is_admin INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS user_keys (\
        id INTEGER PRIMARY KEY, \
        user_id INTEGER NOT NULL, \
        api_key_ciphertext TEXT NOT NULL, \
        api_key_digest TEXT NOT NULL UNIQUE, \
        label TEXT, \
        enabled INTEGER NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    // ── authz ──
    "CREATE TABLE IF NOT EXISTS route_permissions (\
        id INTEGER PRIMARY KEY, \
        scope TEXT NOT NULL, \
        scope_id INTEGER NOT NULL, \
        route_pattern TEXT NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS rate_limits (\
        id INTEGER PRIMARY KEY, \
        scope TEXT NOT NULL, \
        scope_id INTEGER NOT NULL, \
        route_pattern TEXT NOT NULL, \
        rpm INTEGER, \
        rpd INTEGER, \
        total_tokens INTEGER, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS quotas (\
        id INTEGER PRIMARY KEY, \
        scope TEXT NOT NULL, \
        scope_id INTEGER NOT NULL, \
        quota_total TEXT NOT NULL, \
        cost_used TEXT NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL, \
        UNIQUE(scope, scope_id))",
    // ── usage ──
    "CREATE TABLE IF NOT EXISTS usages (\
        id INTEGER PRIMARY KEY, \
        request_id TEXT NOT NULL UNIQUE, \
        at INTEGER NOT NULL, \
        route_name TEXT, \
        provider_id INTEGER, \
        credential_id INTEGER, \
        org_id INTEGER, \
        team_id INTEGER, \
        user_id INTEGER, \
        user_key_id INTEGER, \
        operation TEXT NOT NULL, \
        kind TEXT NOT NULL, \
        model TEXT, \
        input_tokens INTEGER NOT NULL, \
        output_tokens INTEGER NOT NULL, \
        cache_read_tokens INTEGER NOT NULL, \
        cache_creation_5m_tokens INTEGER NOT NULL, \
        cache_creation_1h_tokens INTEGER NOT NULL, \
        cost TEXT NOT NULL, \
        latency_ms INTEGER NOT NULL DEFAULT 0, \
        usage_source TEXT NOT NULL DEFAULT '', \
        ended TEXT NOT NULL DEFAULT '', \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS usage_rollups (\
        id INTEGER PRIMARY KEY, \
        granularity TEXT NOT NULL, \
        bucket_start INTEGER NOT NULL, \
        provider_id INTEGER, \
        org_id INTEGER, \
        team_id INTEGER, \
        user_id INTEGER, \
        route_name TEXT, \
        model TEXT, \
        requests INTEGER NOT NULL, \
        input_tokens INTEGER NOT NULL, \
        output_tokens INTEGER NOT NULL, \
        cost TEXT NOT NULL, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    // One row per dimension bucket — two isolates racing the first insert must
    // collide here (the loser retries into the accumulate path). COALESCE
    // folds NULL dimensions, which unique indexes otherwise treat as distinct.
    "CREATE UNIQUE INDEX IF NOT EXISTS uq_usage_rollups_dims ON usage_rollups (\
        granularity, bucket_start, \
        COALESCE(provider_id, 0), COALESCE(org_id, 0), \
        COALESCE(team_id, 0), COALESCE(user_id, 0), \
        COALESCE(route_name, ''), COALESCE(model, ''))",
    "CREATE TABLE IF NOT EXISTS downstream_requests (\
        id INTEGER PRIMARY KEY, \
        request_id TEXT NOT NULL, \
        at INTEGER NOT NULL, \
        method TEXT NOT NULL, \
        path TEXT NOT NULL, \
        query TEXT, \
        status INTEGER NOT NULL, \
        headers_json TEXT, \
        body TEXT, \
        response_body TEXT, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS upstream_requests (\
        id INTEGER PRIMARY KEY, \
        request_id TEXT NOT NULL, \
        at INTEGER NOT NULL, \
        provider_id INTEGER, \
        credential_id INTEGER, \
        url TEXT NOT NULL, \
        method TEXT NOT NULL, \
        status INTEGER NOT NULL, \
        latency_ms INTEGER NOT NULL, \
        headers_json TEXT, \
        body TEXT, \
        response_body TEXT, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS audit_logs (\
        id INTEGER PRIMARY KEY, \
        at INTEGER NOT NULL, \
        actor_id INTEGER, \
        actor_name TEXT, \
        action TEXT NOT NULL, \
        target TEXT NOT NULL, \
        status INTEGER NOT NULL, \
        source_ip TEXT, \
        created_at INTEGER NOT NULL)",
    // ── settings ──
    "CREATE TABLE IF NOT EXISTS instance_settings (\
        id INTEGER PRIMARY KEY, \
        instance_name TEXT NOT NULL UNIQUE, \
        proxy TEXT, \
        spoof_emulation INTEGER, \
        enable_usage INTEGER NOT NULL, \
        enable_upstream_log INTEGER NOT NULL, \
        enable_upstream_log_body INTEGER NOT NULL, \
        enable_downstream_log INTEGER NOT NULL, \
        enable_downstream_log_body INTEGER NOT NULL, \
        disable_log_redaction INTEGER NOT NULL, \
        enable_tokenizer_download INTEGER NOT NULL, \
        update_channel TEXT, \
        retention_days INTEGER, \
        created_at INTEGER NOT NULL, \
        updated_at INTEGER NOT NULL)",
    // ── tokenizer vocabs ──
    "CREATE TABLE IF NOT EXISTS tokenizer_vocabs (\
        name TEXT PRIMARY KEY, \
        bytes BLOB NOT NULL, \
        updated_at INTEGER NOT NULL)",
];

/// Issue every `CREATE TABLE IF NOT EXISTS`. Each Hrana pipeline call runs one
/// statement, so we iterate.
pub async fn ensure_schema(client: &LibsqlClient) -> anyhow::Result<()> {
    for sql in TABLES {
        client
            .execute(sql, &[])
            .await
            .map_err(|e| anyhow::anyhow!("libsql ensure_schema failed: {e}"))?;
    }
    run_migrations(client).await
}

/// Stamp an unstamped DB at the latest version, then apply pending ordered
/// migrations — mirroring the `db` backend over `LibsqlClient::execute`. Runs
/// after [`ensure_schema`]'s `CREATE TABLE IF NOT EXISTS`, so an empty
/// `schema_migrations` means the tables already hold the *current* schema; it
/// is stamped at [`latest_version`] without replaying any DDL.
async fn run_migrations(client: &LibsqlClient) -> anyhow::Result<()> {
    let exec = |sql: String| async move {
        client
            .execute(&sql, &[])
            .await
            .map_err(|e| anyhow::anyhow!("libsql migration failed: {e}"))
    };

    exec(CREATE_MIGRATIONS_TABLE.to_string()).await?;

    let qr = client
        .execute(SELECT_MAX_VERSION, &[])
        .await
        .map_err(|e| anyhow::anyhow!("libsql read schema version failed: {e}"))?;
    let current = match qr.rows.first() {
        Some(row) => col_i64(row, 0)?,
        None => 0,
    };

    let current = if current == 0 {
        let latest = latest_version();
        record_version(client, latest).await?;
        latest
    } else {
        current
    };

    for m in pending(current) {
        for sql in m.sql {
            exec((*sql).to_string()).await?;
        }
        record_version(client, m.version).await?;
    }
    Ok(())
}

async fn record_version(client: &LibsqlClient, version: i64) -> anyhow::Result<()> {
    let now = (js_sys::Date::now() / 1000.0) as i64;
    client
        .execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)",
            &[
                crate::store::libsql::arg_integer(version),
                crate::store::libsql::arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql record version failed: {e}"))?;
    Ok(())
}
