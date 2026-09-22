//! Reading a v3 SQLite file directly, read-only.
//!
//! # Why this is the primary route, and the export is the second one
//!
//! v3 has no `export` subcommand. Its binary's only subcommand is `migrate`,
//! which imports a **v2** database; `gproxy export --help` on v3.0.16 answers
//! `unrecognized subcommand`. An export document exists, but only behind
//! `POST /admin/api/export` (`v3:crates/gproxy-admin/src/route.rs`), so
//! producing one needs a **running** v3 instance and an administrator session.
//! An operator holding a `gproxy.db` and a stopped service cannot make one.
//!
//! So the file is the route, and v3 set the precedent for exactly this problem
//! one version earlier: `v3:crates/gproxy-app/src/migrate_v2/` opens a v2
//! database and translates out of it. This module is the same idea with the
//! versions moved on by one.
//!
//! # Read-only means read-only
//!
//! The connection is opened `mode=ro`, so the driver refuses a write rather
//! than relying on this module not to attempt one. Nothing here issues DDL,
//! nothing writes a marker into the source, and the file is left byte for byte
//! as it was — which is what makes the migration rollback-able: if the result
//! is wrong, the v3 service starts again on the same file.
//!
//! An operator should still **stop the v3 service first**. `mode=ro` protects
//! the file from this process, not from a concurrent writer: reading a database
//! mid-transaction can produce a torn view, and for the OAuth channels it is
//! worse than torn — `claudecode` and `codex` rotate their refresh tokens on
//! use, so a second instance that starts up and refreshes one invalidates the
//! copy the original is still holding.
//!
//! # What is read, and what is deliberately not
//!
//! Every configuration and identity table, and none of the history. In
//! production the history is most of the database — 200767
//! `credential_quota_observations`, 91242 `request_logs`, 88255 `wire_logs`,
//! 83093 `usage_rows` — and none of it is configuration. See the deployment
//! page for the full list of what an operator loses.
//!
//! # The shapes that are not the export's
//!
//! Three tables are stored differently from the way v3's admin API serves
//! them, so this module converts rather than deserializing:
//!
//! - `rules` keeps `kind` in a column and the rest in `config_json`, where the
//!   DTO nests them; and its `transform` locate is `{"path": …}` where the DTO
//!   writes `{"type": "path", "value": …}`
//!   (`v3:crates/gproxy-admin/src/dto/rules.rs`, `RuleConfigDto::storage`).
//! - `rules.filter_operations_json` is a JSON array in a text column.
//! - `settings` is a key/value table, which the export omits entirely.

use std::{collections::BTreeMap, path::Path};

use gproxy_seaorm::SchemaSyncConnectionTrait;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, QueryResult, Statement};
use serde_json::{Value, json};

use super::document::{self, Document};
use crate::{Error, Result};

/// Open a v3 database read-only and read everything the migration needs.
pub async fn read(path: &Path) -> Result<Document> {
    if !path.is_file() {
        return Err(Error::other(format!(
            "{} is not a readable file. --from-v3 takes either a v3 SQLite database or a \
             document from v3's `POST /admin/api/export`.",
            path.display()
        )));
    }
    let connection = open(path).await?;
    // The same check every other entry point makes, for the opposite reason:
    // here a database that is *not* v3's is the error.
    match super::detect::inspect(&connection).await? {
        super::detect::Verdict::Version3 { .. } => {}
        _ => {
            return Err(Error::other(format!(
                "{} is not a GPROXY v3 database: it carries no `schema_migrations` ledger. \
                 --from-v3 reads a v3 SQLite file or a v3 export document; a v4 database is \
                 already v4.",
                path.display()
            )));
        }
    }
    let data = data(&connection).await?;
    Ok(Document {
        format_version: document::FORMAT_VERSION,
        // The file always has its secrets; only the admin API's export can
        // omit them.
        secrets: document::Secrets::Included,
        source_key: Some(source_key(&data)),
        data,
    })
}

/// Whether this file's secrets are sealed, decided from **the rows**.
///
/// The obvious check is `settings.master_key_fingerprint`, and it is wrong: v3
/// writes that key with a `null` value when it has no master key, so its
/// presence says nothing. The local development database is exactly that — the
/// key is there, its value is `null`, and every credential's `wrapped_key`,
/// `payload_nonce` and `key_nonce` are empty.
///
/// So the envelopes decide. A sealed one carries a wrapped key and two nonces;
/// an unsealed one is bare JSON in `ciphertext` with the other three columns
/// empty (`v3:crates/gproxy-app/src/secrets.rs`). One sealed row anywhere means
/// the instance had a key, which is what `--source-master-key` has to match.
fn source_key(data: &document::Data) -> document::SourceKey {
    let sealed = data
        .credentials
        .iter()
        .filter_map(|row| row.secret.as_ref())
        .chain(data.user_keys.iter().filter_map(|row| row.secret.as_ref()))
        .any(|envelope| !envelope.is_plaintext());
    match sealed {
        true => document::SourceKey::Sealed {
            fingerprint: data
                .settings
                .get(MASTER_KEY_FINGERPRINT)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        },
        false => document::SourceKey::Plaintext,
    }
}

/// The setting v3 writes when it has a master key — and also, with a `null`
/// value, when it does not. Only good for the fingerprint text; see
/// [`source_key`].
const MASTER_KEY_FINGERPRINT: &str = "master_key_fingerprint";

async fn open(path: &Path) -> Result<DatabaseConnection> {
    // `mode=ro`: the driver refuses a write, rather than this module promising
    // not to attempt one.
    let url = format!("sqlite://{}?mode=ro", path.to_string_lossy());
    Database::connect(&url).await.map_err(|error| {
        Error::other(format!(
            "could not open {} read-only: {error}",
            path.display()
        ))
    })
}

async fn data(connection: &DatabaseConnection) -> Result<document::Data> {
    let tables = connection.table_names().await?;
    Ok(document::Data {
        organizations: rows(connection, &tables, "organizations", organization).await?,
        teams: rows(connection, &tables, "teams", team).await?,
        users: rows(connection, &tables, "users", user).await?,
        providers: rows(connection, &tables, "providers", provider).await?,
        credentials: rows(connection, &tables, "credentials", credential).await?,
        user_keys: rows(connection, &tables, "user_keys", user_key).await?,
        quotas: rows(connection, &tables, "quotas", quota).await?,
        price_rules: rows(connection, &tables, "price_rules", price_rule).await?,
        price_rates: rows(connection, &tables, "price_rates", price_rate).await?,
        routes: rows(connection, &tables, "routes", route).await?,
        route_members: rows(connection, &tables, "route_members", route_member).await?,
        aliases: rows(connection, &tables, "aliases", alias).await?,
        model_aliases: rows(connection, &tables, "exposed_models", model_alias).await?,
        routing_rules: rows(connection, &tables, "routing_rules", routing_rule).await?,
        rule_sets: rows(connection, &tables, "rule_sets", rule_set).await?,
        rules: rows(connection, &tables, "rules", rule).await?,
        provider_rule_sets: rows(connection, &tables, "provider_rule_sets", provider_rule_set)
            .await?,
        // The three v3's own export forgets are ordinary tables in the file,
        // so the direct route has them without any splicing.
        permissions: rows(connection, &tables, "permissions", permission).await?,
        rate_limits: rows(connection, &tables, "rate_limits", rate_limit).await?,
        provider_models: rows(connection, &tables, "provider_models", provider_model).await?,
        settings: settings(connection).await?,
    })
}

/// Every table this module reads. `SELECT *`, not a column list, and every
/// value is then taken **by name**.
///
/// That is not laziness, it is the one thing this module has to get right. v3's
/// schema grew over ten versions and an operator's file can be at any of them:
/// the local development database here is at version 7, where `routes` has no
/// `strategy` (added at 9) and `permissions` has no `model_pattern` (added at
/// 8), while production is at 10 and has both. A fixed column list fails on
/// `no such column: strategy` against a database that is *perfectly valid v3*
/// and whose configuration migrates fine without it.
///
/// So a column that is not there reads as absent and the row takes v3's own
/// default for it, and only a column that has existed since version 1 is
/// required.
const TABLES: [(&str, &str); 20] = [
    ("organizations", "SELECT * FROM organizations ORDER BY id"),
    ("teams", "SELECT * FROM teams ORDER BY id"),
    ("users", "SELECT * FROM users ORDER BY id"),
    ("providers", "SELECT * FROM providers ORDER BY id"),
    ("credentials", "SELECT * FROM credentials ORDER BY id"),
    ("user_keys", "SELECT * FROM user_keys ORDER BY id"),
    ("quotas", "SELECT * FROM quotas ORDER BY id"),
    ("price_rules", "SELECT * FROM price_rules ORDER BY id"),
    ("price_rates", "SELECT * FROM price_rates ORDER BY id"),
    ("routes", "SELECT * FROM routes ORDER BY id"),
    ("route_members", "SELECT * FROM route_members ORDER BY id"),
    ("aliases", "SELECT * FROM aliases ORDER BY id"),
    ("exposed_models", "SELECT * FROM exposed_models ORDER BY id"),
    ("routing_rules", "SELECT * FROM routing_rules ORDER BY id"),
    ("rule_sets", "SELECT * FROM rule_sets ORDER BY id"),
    ("rules", "SELECT * FROM rules ORDER BY id"),
    (
        "provider_rule_sets",
        "SELECT * FROM provider_rule_sets ORDER BY id",
    ),
    ("permissions", "SELECT * FROM permissions ORDER BY id"),
    ("rate_limits", "SELECT * FROM rate_limits ORDER BY id"),
    (
        "provider_models",
        "SELECT * FROM provider_models ORDER BY id",
    ),
];

fn statement_for(table: &str) -> &'static str {
    TABLES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, sql)| *sql)
        .expect("every table read here is in TABLES")
}

const SETTINGS: &str = "SELECT key, value_json FROM settings ORDER BY key";

/// Run one table's statement and convert each row.
///
/// A table that is not there at all is empty rather than fatal: v3 created
/// every one of these at schema version 1, but a hand-trimmed database is a
/// thing operators produce, and refusing to migrate the other nineteen tables
/// because of it would help nobody.
async fn rows<T>(
    connection: &DatabaseConnection,
    tables: &[String],
    table: &'static str,
    convert: fn(&QueryResult) -> Result<T>,
) -> Result<Vec<T>> {
    if !tables.iter().any(|name| name == table) {
        tracing::warn!(
            table,
            "this v3 database has no such table; migrating without it"
        );
        return Ok(Vec::new());
    }
    let statement = Statement::from_string(connection.get_database_backend(), statement_for(table));
    let found = connection.query_all_raw(statement).await?;
    found
        .iter()
        .map(convert)
        .collect::<Result<Vec<T>>>()
        .map_err(|error| Error::other(format!("reading v3 `{table}`: {error}")))
}

// ------------------------------------------------------- column readers --
//
// Everything is taken by **name**, and the optional readers treat "no such
// column" exactly as they treat NULL. That is what lets one reader cover v3's
// ten schema versions: a column added at version 8 is simply absent in a
// version 7 file, and the row takes v3's own default for it.
//
// Only a column that has existed since version 1 uses a required reader, and a
// required column that is missing is a real error — a `users` table with no
// `name` is not an old v3 database, it is a damaged one.

/// A column that has existed since v3's first schema version.
fn integer(row: &QueryResult, column: &str) -> Result<i64> {
    row.try_get::<i64>("", column)
        .map_err(|error| Error::other(format!("column `{column}` is not an integer: {error}")))
}

/// Absent, NULL, or a value. The first two are the same answer.
fn optional_integer(row: &QueryResult, column: &str) -> Option<i64> {
    row.try_get::<Option<i64>>("", column).ok().flatten()
}

fn text(row: &QueryResult, column: &str) -> Result<String> {
    row.try_get::<String>("", column)
        .map_err(|error| Error::other(format!("column `{column}` is not text: {error}")))
}

fn optional_text(row: &QueryResult, column: &str) -> Option<String> {
    row.try_get::<Option<String>>("", column).ok().flatten()
}

fn blob(row: &QueryResult, column: &str) -> Result<Vec<u8>> {
    row.try_get::<Vec<u8>>("", column)
        .map_err(|error| Error::other(format!("column `{column}` is not a blob: {error}")))
}

fn optional_blob(row: &QueryResult, column: &str) -> Option<Vec<u8>> {
    row.try_get::<Option<Vec<u8>>>("", column).ok().flatten()
}

/// v3 stored every boolean as an integer. `default` is what v3's own column
/// default was, for a file predating the column.
fn flag(row: &QueryResult, column: &str, default: bool) -> bool {
    optional_integer(row, column).map_or(default, |value| value != 0)
}

/// A JSON text column. v3 wrote valid JSON into every one of them, but a
/// hand-edited row is possible and losing the whole migration to one is not
/// worth it: an unparseable value becomes null and the row keeps its shape.
fn json(row: &QueryResult, column: &str) -> Value {
    optional_text(row, column)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

/// A decimal kept as text, which is how v3 stored money.
fn decimal(row: &QueryResult, column: &str) -> Option<String> {
    optional_text(row, column)
}

/// An integer column narrowed to `u32`, with v3's default for an absent one.
fn count(row: &QueryResult, column: &str, default: u32) -> u32 {
    optional_integer(row, column)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(default)
}

// --------------------------------------------------------- row readers --
fn organization(row: &QueryResult) -> Result<document::Organization> {
    Ok(document::Organization {
        id: integer(row, "id")?,
        name: text(row, "name")?,
        enabled: flag(row, "enabled", true),
    })
}

fn team(row: &QueryResult) -> Result<document::Team> {
    Ok(document::Team {
        id: integer(row, "id")?,
        organization_id: integer(row, "organization_id")?,
        name: text(row, "name")?,
        enabled: flag(row, "enabled", true),
    })
}

fn user(row: &QueryResult) -> Result<document::User> {
    Ok(document::User {
        password_hash: optional_text(row, "password_hash"),
        id: integer(row, "id")?,
        name: text(row, "name")?,
        organization_id: optional_integer(row, "organization_id"),
        team_id: optional_integer(row, "team_id"),
        enabled: flag(row, "enabled", true),
        // Added at schema version 1 with a `0` default.
        is_admin: flag(row, "is_admin", false),
    })
}

fn provider(row: &QueryResult) -> Result<document::Provider> {
    Ok(document::Provider {
        id: integer(row, "id")?,
        name: text(row, "name")?,
        channel: text(row, "channel")?,
        settings: json(row, "settings_json"),
        enabled: flag(row, "enabled", true),
        label: optional_text(row, "label"),
        proxy_url: optional_text(row, "proxy_url"),
        credential_strategy: optional_text(row, "credential_strategy"),
        // v3's TLS fingerprint and traffic policy have no v4 form; they are
        // not read rather than read and dropped.
        tls_fingerprint: None,
        traffic_policy: None,
    })
}

fn credential(row: &QueryResult) -> Result<document::Credential> {
    Ok(document::Credential {
        config: document::CredentialConfig {
            id: integer(row, "id")?,
            provider_id: integer(row, "provider_id")?,
            label: optional_text(row, "label"),
            version: optional_integer(row, "version").unwrap_or(1).max(0) as u64,
            enabled: flag(row, "enabled", true),
            // v3's column default.
            weight: count(row, "weight", 100),
            rpm_limit: optional_integer(row, "rpm_limit").and_then(|v| u32::try_from(v).ok()),
            tpm_limit: optional_integer(row, "tpm_limit").and_then(|v| u64::try_from(v).ok()),
            proxy_url: optional_text(row, "proxy_url"),
            // v3's column default, for a file predating the column.
            kind: optional_text(row, "kind").unwrap_or_else(|| "api_key".to_owned()),
        },
        secret: Some(document::Envelope {
            ciphertext: blob(row, "ciphertext")?,
            wrapped_key: blob(row, "wrapped_key")?,
            payload_nonce: blob(row, "payload_nonce")?,
            key_nonce: blob(row, "key_nonce")?,
        }),
    })
}

fn user_key(row: &QueryResult) -> Result<document::UserKey> {
    // Nullable in v3: only a key created as revealable kept its text.
    let secret = optional_blob(row, "ciphertext").map(|ciphertext| document::Envelope {
        ciphertext,
        wrapped_key: optional_blob(row, "wrapped_key").unwrap_or_default(),
        payload_nonce: optional_blob(row, "payload_nonce").unwrap_or_default(),
        key_nonce: optional_blob(row, "key_nonce").unwrap_or_default(),
    });
    Ok(document::UserKey {
        config: document::UserKeyConfig {
            id: integer(row, "id")?,
            user_id: integer(row, "user_id")?,
            prefix: optional_text(row, "prefix"),
            label: optional_text(row, "label"),
            expires_at: optional_integer(row, "expires_at"),
            enabled: flag(row, "enabled", true),
        },
        digest: blob(row, "digest")?,
        // v3's column default, and the only version it ever wrote.
        digest_version: count(row, "digest_version", document::DIGEST_VERSION),
        secret,
    })
}

fn quota(row: &QueryResult) -> Result<document::Quota> {
    Ok(document::Quota {
        id: integer(row, "id")?,
        subject_kind: text(row, "subject_kind")?,
        subject_id: integer(row, "subject_id")?,
        quota_total: decimal(row, "quota_total"),
        quota_daily: decimal(row, "quota_daily"),
        quota_weekly: decimal(row, "quota_weekly"),
        quota_monthly: decimal(row, "quota_monthly"),
        quota_5h: decimal(row, "quota_5h"),
        quota_7d: decimal(row, "quota_7d"),
        enabled: flag(row, "enabled", true),
    })
}

fn price_rule(row: &QueryResult) -> Result<document::PriceRule> {
    Ok(document::PriceRule {
        id: integer(row, "id")?,
        provider_id: optional_integer(row, "provider_id"),
        model_pattern: text(row, "model_pattern")?,
        tiers: match json(row, "tiers_json") {
            Value::Null => None,
            value => Some(value),
        },
        priority: optional_integer(row, "priority").unwrap_or_default(),
        enabled: flag(row, "enabled", true),
    })
}

fn price_rate(row: &QueryResult) -> Result<document::PriceRate> {
    Ok(document::PriceRate {
        id: integer(row, "id")?,
        rule_id: integer(row, "rule_id")?,
        metric: text(row, "metric")?,
        unit_size: optional_integer(row, "unit_size")
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(1),
        price: text(row, "price")?,
        conditions: match json(row, "conditions_json") {
            Value::Null => None,
            value => Some(value),
        },
        priority: optional_integer(row, "priority").unwrap_or_default(),
    })
}

fn route(row: &QueryResult) -> Result<document::Route> {
    Ok(document::Route {
        id: integer(row, "id")?,
        name: text(row, "name")?,
        // Added at schema version 9 (`RouteStrategies`) with a `weighted`
        // default, and absent from every earlier file — including the local
        // development database, which is at version 7.
        strategy: optional_text(row, "strategy"),
        max_attempts: count(row, "max_attempts", 1),
        enabled: flag(row, "enabled", true),
    })
}

fn route_member(row: &QueryResult) -> Result<document::RouteMember> {
    Ok(document::RouteMember {
        id: integer(row, "id")?,
        route_id: integer(row, "route_id")?,
        provider_id: integer(row, "provider_id")?,
        upstream_model: text(row, "upstream_model")?,
        tier: count(row, "tier", 0),
        weight: count(row, "weight", 100),
        enabled: flag(row, "enabled", true),
    })
}

fn alias(row: &QueryResult) -> Result<document::Alias> {
    Ok(document::Alias {
        id: integer(row, "id")?,
        alias: text(row, "alias")?,
        target: text(row, "target")?,
        provider_id: optional_integer(row, "provider_id"),
        priority: optional_integer(row, "priority").unwrap_or_default(),
        enabled: flag(row, "enabled", true),
    })
}

fn model_alias(row: &QueryResult) -> Result<document::ModelAlias> {
    Ok(document::ModelAlias {
        id: integer(row, "id")?,
        name: text(row, "name")?,
        route_id: integer(row, "route_id")?,
        enabled: flag(row, "enabled", true),
    })
}

fn routing_rule(row: &QueryResult) -> Result<document::RoutingRule> {
    Ok(document::RoutingRule {
        id: integer(row, "id")?,
        provider_id: integer(row, "provider_id")?,
        operation: text(row, "operation")?,
        kind: text(row, "kind")?,
        implementation: text(row, "implementation")?,
        dest_operation: optional_text(row, "dest_operation"),
        dest_kind: optional_text(row, "dest_kind"),
        sort_order: optional_integer(row, "sort_order").unwrap_or_default(),
        enabled: flag(row, "enabled", true),
        // The column the export DTO has no field for, and the one that decides
        // whether a row is operator intent or a seeded channel default.
        origin: optional_text(row, "origin"),
    })
}

fn rule_set(row: &QueryResult) -> Result<document::RuleSet> {
    Ok(document::RuleSet {
        id: integer(row, "id")?,
        name: text(row, "name")?,
        description: optional_text(row, "description"),
        enabled: flag(row, "enabled", true),
    })
}

/// v3's `rules` row into the nested shape the admin API serves, which is what
/// the rest of the migration reads. The `kind` column and `config_json` are one
/// tagged object there; the locate is `{"type": …, "value": …}` there and
/// `{"path": …}` here.
fn rule(row: &QueryResult) -> Result<document::Rule> {
    let id = integer(row, "id")?;
    let kind = text(row, "kind")?;
    let stored = json(row, "config_json");
    let config = rule_config(&kind, &stored).ok_or_else(|| {
        Error::other(format!(
            "rule {id} has kind `{kind}` with a config this migration cannot read. v3 wrote \
             five kinds: system_text, cache_breakpoint, rewrite, transform, header."
        ))
    })?;
    Ok(document::Rule {
        id,
        rule_set_id: integer(row, "rule_set_id")?,
        config,
        filter_model_pattern: optional_text(row, "filter_model_pattern"),
        filter_operations: match json(row, "filter_operations_json") {
            Value::Array(items) => Some(
                items
                    .into_iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect(),
            ),
            _ => None,
        },
        filter_header_pattern: optional_text(row, "filter_header_pattern"),
        sort_order: optional_integer(row, "sort_order").unwrap_or_default(),
        enabled: flag(row, "enabled", true),
    })
}

/// The stored config of one rule as the tagged enum the rest reads.
fn rule_config(kind: &str, stored: &Value) -> Option<document::RuleConfig> {
    let mut tagged = stored.as_object()?.clone();
    tagged.insert("kind".into(), Value::String(kind.to_owned()));
    match kind {
        // v3's storage writes `value_json` where the DTO reads `value`.
        "rewrite" => {
            if let Some(value) = tagged.remove("value_json") {
                tagged.insert("value".into(), value);
            }
        }
        // `{"path": x}` / `{"paths": [..]}` / `{"match": x}` become the DTO's
        // externally tagged `{"type": …, "value": …}`.
        "transform" => {
            let locate = tagged.get("locate")?.as_object()?.clone();
            let (kind, value) = ["path", "paths", "match"]
                .into_iter()
                .find_map(|name| locate.get(name).map(|value| (name, value.clone())))?;
            tagged.insert("locate".into(), json!({"type": kind, "value": value}));
        }
        _ => {}
    }
    serde_json::from_value(Value::Object(tagged)).ok()
}

fn provider_rule_set(row: &QueryResult) -> Result<document::ProviderRuleSet> {
    Ok(document::ProviderRuleSet {
        id: integer(row, "id")?,
        provider_id: integer(row, "provider_id")?,
        rule_set_id: integer(row, "rule_set_id")?,
        sort_order: optional_integer(row, "sort_order").unwrap_or_default(),
        enabled: flag(row, "enabled", true),
    })
}

fn permission(row: &QueryResult) -> Result<document::Permission> {
    Ok(document::Permission {
        id: integer(row, "id")?,
        subject_kind: text(row, "subject_kind")?,
        subject_id: integer(row, "subject_id")?,
        provider_id: optional_integer(row, "provider_id"),
        operation_group: optional_text(row, "operation_group"),
        // Added at schema version 8 (`ModelPermissions`); absent in a version
        // 7 file, where a rule covered every model.
        model_pattern: optional_text(row, "model_pattern"),
        allowed: flag(row, "allowed", false),
    })
}

fn rate_limit(row: &QueryResult) -> Result<document::RateLimit> {
    Ok(document::RateLimit {
        id: integer(row, "id")?,
        subject_kind: text(row, "subject_kind")?,
        subject_id: integer(row, "subject_id")?,
        requests: optional_integer(row, "requests")
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or_default(),
        window_seconds: optional_integer(row, "window_seconds")
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(60),
    })
}

fn provider_model(row: &QueryResult) -> Result<document::ProviderModel> {
    Ok(document::ProviderModel {
        id: integer(row, "id")?,
        provider_id: integer(row, "provider_id")?,
        model_id: text(row, "model_id")?,
        display_name: optional_text(row, "display_name"),
        // v3 spread a model's capabilities over forty columns and five side
        // tables. Only the two v4's metadata has a home for are read; the rest
        // is reported as not migrated.
        context_window: optional_integer(row, "context_window")
            .or_else(|| optional_integer(row, "max_context_window")),
        max_output_tokens: optional_integer(row, "max_output_tokens"),
        metadata: Value::Null,
        enabled: flag(row, "enabled", true),
    })
}

/// v3's key/value settings table. The export omits it entirely, so this is the
/// only route that carries any of it.
async fn settings(connection: &DatabaseConnection) -> Result<BTreeMap<String, Value>> {
    let statement = Statement::from_string(connection.get_database_backend(), SETTINGS.to_owned());
    let found = connection
        .query_all_raw(statement)
        .await
        .map_err(|error| Error::other(format!("reading v3 settings: {error}")))?;
    let mut out = BTreeMap::new();
    for row in &found {
        out.insert(text(row, "key")?, json(row, "value_json"));
    }
    Ok(out)
}

/// The settings key this migration writes into the **destination**, so that a
/// second run against the same source is a no-op rather than a second set of
/// rows.
///
/// The scheme is v3's own, moved on by one: it wrote
/// `v2_import_{sha256(canonical path)}` after importing a v2 database
/// (`v3:crates/gproxy-app/src/migrate_v2/mod.rs`, `source_marker`). Keyed by
/// path rather than by content because the content changes as the v3 instance
/// keeps running, and "have I already imported *that file*" is the question an
/// operator is asking.
pub fn source_marker(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let resolved = path
        .canonicalize()
        .map_err(|error| Error::other(format!("could not resolve {}: {error}", path.display())))?;
    let digest = Sha256::digest(resolved.as_os_str().as_encoded_bytes());
    Ok(format!("v3_import_{}", crate::v3::hex(&digest)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v3's storage shape for each of the five rule kinds, as
    /// `RuleConfigDto::storage` writes them.
    #[test]
    fn every_stored_rule_kind_becomes_the_tagged_shape_the_migration_reads() {
        let cases = [
            ("system_text", json!({"text": "hi", "position": "append"})),
            (
                "cache_breakpoint",
                json!({"target": "system", "index": 1, "ttl": "5m"}),
            ),
            // The renamed field.
            (
                "rewrite",
                json!({"path": "a.b", "action": "set", "value_json": {"x": 1}}),
            ),
            (
                "header",
                json!({"name": "x-a", "value": "b", "mode": "override"}),
            ),
        ];
        for (kind, stored) in cases {
            let config =
                rule_config(kind, &stored).unwrap_or_else(|| panic!("`{kind}` did not convert"));
            assert_eq!(config.kind(), kind);
        }
    }

    /// The locate shapes, which are the reason this is a conversion and not a
    /// deserialization.
    #[test]
    fn a_stored_transform_locate_becomes_the_dtos_tagged_form() {
        for locate in [
            json!({"path": "a.b"}),
            json!({"paths": ["a.b", "c"]}),
            json!({"match": "x+"}),
        ] {
            let stored = json!({
                "phase": "request",
                "locate": locate,
                "actions": [{"op": "replace_regex", "pattern": "x", "with": "y"}],
                "limit": null,
            });
            let config = rule_config("transform", &stored).expect("converts");
            let document::RuleConfig::Transform { locate, .. } = config else {
                panic!("not a transform");
            };
            // Each of the three arms, distinguishable from the others.
            match locate {
                document::Locate::Path(path) => assert_eq!(path, "a.b"),
                document::Locate::Paths(paths) => assert_eq!(paths, ["a.b", "c"]),
                document::Locate::Match(pattern) => assert_eq!(pattern, "x+"),
            }
        }
    }

    #[test]
    fn a_rule_kind_v3_never_wrote_is_not_silently_accepted() {
        assert!(rule_config("something_else", &json!({})).is_none());
        // And a transform whose locate is none of the three shapes.
        assert!(
            rule_config(
                "transform",
                &json!({"phase": "request", "locate": {"nope": 1}, "actions": []})
            )
            .is_none()
        );
    }

    #[test]
    fn the_marker_is_v3s_own_scheme_with_the_version_moved_on() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let marker = source_marker(file.path()).unwrap();
        assert!(marker.starts_with("v3_import_"), "{marker}");
        // 64 hex characters of SHA-256, like v3's.
        assert_eq!(marker.len(), "v3_import_".len() + 64);
        // Deterministic for one path, and different for another.
        assert_eq!(marker, source_marker(file.path()).unwrap());
        let other = tempfile::NamedTempFile::new().unwrap();
        assert_ne!(marker, source_marker(other.path()).unwrap());
    }

    #[tokio::test]
    async fn a_path_that_is_not_a_file_says_what_from_v3_accepts() {
        let error = read(Path::new("/nonexistent/gproxy.db"))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a readable file"), "{error}");
        assert!(error.contains("export"), "{error}");
    }
}
