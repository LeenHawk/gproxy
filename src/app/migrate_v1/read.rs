//! MIGRATE-V1 (remove in 2.1): read the legacy v1 SQLite database (read-only)
//! into plain row structs. Only the control-plane config tables are read; usage,
//! request logs, files and ephemeral health are intentionally skipped (§ design).

use std::path::Path;

use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

/// Open the v1 database read-only (never mutated — it stays as the backup).
/// Not `immutable`: v1 ran in WAL mode, and immutable mode would skip any
/// uncheckpointed `-wal` data. The v1 process is stopped during migration, so a
/// read-only connection builds an in-memory wal-index and sees the full data.
pub async fn open_ro(path: &Path) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::new().filename(path).read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .map_err(|e| anyhow::anyhow!("open v1 db {}: {e}", path.display()))?;
    Ok(pool)
}

/// Does a table exist in the opened SQLite db? Used to sniff v1 vs v2 schemas.
pub async fn table_exists(pool: &SqlitePool, name: &str) -> anyhow::Result<bool> {
    let row = sqlx::query("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub struct V1User {
    pub id: i64,
    pub name: String,
    pub password: Option<String>,
    pub enabled: bool,
    pub is_admin: bool,
}

pub struct V1UserKey {
    pub id: i64,
    pub user_id: i64,
    pub api_key_ciphertext: String,
    pub label: Option<String>,
    pub enabled: bool,
}

pub struct V1Provider {
    pub id: i64,
    pub name: String,
    pub channel: String,
    pub label: Option<String>,
    pub settings_json: String,
}

pub struct V1Credential {
    pub id: i64,
    pub provider_id: i64,
    pub name: Option<String>,
    pub kind: String,
    pub secret_json: String,
    pub enabled: bool,
}

pub struct V1Model {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    pub pricing_json: Option<String>,
}

pub struct V1Quota {
    pub user_id: i64,
    pub quota: f64,
    pub cost_used: f64,
}

pub struct V1ModelPerm {
    pub user_id: i64,
    pub model_pattern: String,
}

pub struct V1RateLimit {
    pub user_id: i64,
    pub model_pattern: String,
    pub rpm: Option<i64>,
    pub rpd: Option<i64>,
    pub total_tokens: Option<i64>,
}

pub struct V1GlobalSettings {
    pub proxy: Option<String>,
    pub spoof_emulation: Option<String>,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub update_channel: Option<String>,
}

/// All control-plane config read out of a v1 database.
#[derive(Default)]
pub struct V1Data {
    pub users: Vec<V1User>,
    pub user_keys: Vec<V1UserKey>,
    pub providers: Vec<V1Provider>,
    pub credentials: Vec<V1Credential>,
    pub models: Vec<V1Model>,
    pub quotas: Vec<V1Quota>,
    pub model_perms: Vec<V1ModelPerm>,
    pub rate_limits: Vec<V1RateLimit>,
    pub settings: Option<V1GlobalSettings>,
}

/// Read every migrated table from the v1 database.
pub async fn read_all(pool: &SqlitePool) -> anyhow::Result<V1Data> {
    let mut data = V1Data::default();

    for r in sqlx::query("SELECT id, name, password, enabled, is_admin FROM users")
        .fetch_all(pool)
        .await?
    {
        data.users.push(V1User {
            id: r.try_get("id")?,
            name: r.try_get("name")?,
            password: r.try_get("password")?,
            enabled: r.try_get("enabled")?,
            is_admin: r.try_get("is_admin")?,
        });
    }

    for r in sqlx::query("SELECT id, user_id, api_key_ciphertext, label, enabled FROM user_keys")
        .fetch_all(pool)
        .await?
    {
        data.user_keys.push(V1UserKey {
            id: r.try_get("id")?,
            user_id: r.try_get("user_id")?,
            api_key_ciphertext: r.try_get("api_key_ciphertext")?,
            label: r.try_get("label")?,
            enabled: r.try_get("enabled")?,
        });
    }

    for r in sqlx::query("SELECT id, name, channel, label, settings_json FROM providers")
        .fetch_all(pool)
        .await?
    {
        data.providers.push(V1Provider {
            id: r.try_get("id")?,
            name: r.try_get("name")?,
            channel: r.try_get("channel")?,
            label: r.try_get("label")?,
            settings_json: r.try_get("settings_json")?,
        });
    }

    for r in
        sqlx::query("SELECT id, provider_id, name, kind, secret_json, enabled FROM credentials")
            .fetch_all(pool)
            .await?
    {
        data.credentials.push(V1Credential {
            id: r.try_get("id")?,
            provider_id: r.try_get("provider_id")?,
            name: r.try_get("name")?,
            kind: r.try_get("kind")?,
            secret_json: r.try_get("secret_json")?,
            enabled: r.try_get("enabled")?,
        });
    }

    for r in sqlx::query(
        "SELECT id, provider_id, model_id, display_name, enabled, pricing_json FROM models",
    )
    .fetch_all(pool)
    .await?
    {
        data.models.push(V1Model {
            id: r.try_get("id")?,
            provider_id: r.try_get("provider_id")?,
            model_id: r.try_get("model_id")?,
            display_name: r.try_get("display_name")?,
            enabled: r.try_get("enabled")?,
            pricing_json: r.try_get("pricing_json")?,
        });
    }

    for r in sqlx::query("SELECT user_id, quota, cost_used FROM user_quotas")
        .fetch_all(pool)
        .await?
    {
        data.quotas.push(V1Quota {
            user_id: r.try_get("user_id")?,
            quota: r.try_get("quota")?,
            cost_used: r.try_get("cost_used")?,
        });
    }

    for r in sqlx::query("SELECT user_id, model_pattern FROM user_model_permissions")
        .fetch_all(pool)
        .await?
    {
        data.model_perms.push(V1ModelPerm {
            user_id: r.try_get("user_id")?,
            model_pattern: r.try_get("model_pattern")?,
        });
    }

    for r in
        sqlx::query("SELECT user_id, model_pattern, rpm, rpd, total_tokens FROM user_rate_limits")
            .fetch_all(pool)
            .await?
    {
        data.rate_limits.push(V1RateLimit {
            user_id: r.try_get("user_id")?,
            model_pattern: r.try_get("model_pattern")?,
            rpm: r.try_get("rpm")?,
            rpd: r.try_get("rpd")?,
            total_tokens: r.try_get("total_tokens")?,
        });
    }

    if let Some(r) = sqlx::query(
        "SELECT proxy, spoof_emulation, enable_usage, enable_upstream_log, \
         enable_upstream_log_body, enable_downstream_log, enable_downstream_log_body, \
         update_channel FROM global_settings LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    {
        data.settings = Some(V1GlobalSettings {
            proxy: r.try_get("proxy")?,
            spoof_emulation: r.try_get("spoof_emulation")?,
            enable_usage: r.try_get("enable_usage")?,
            enable_upstream_log: r.try_get("enable_upstream_log")?,
            enable_upstream_log_body: r.try_get("enable_upstream_log_body")?,
            enable_downstream_log: r.try_get("enable_downstream_log")?,
            enable_downstream_log_body: r.try_get("enable_downstream_log_body")?,
            update_channel: r.try_get("update_channel")?,
        });
    }

    Ok(data)
}
