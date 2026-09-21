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
    match super::detect::inspect(&connection).await {
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
        // Whether they are sealed is decided by what is in the rows, not by a
        // marker: a v3 instance with no master key wrote plain JSON into the
        // same columns. `settings.master_key_fingerprint` says which, and is
        // read below into `data.settings`.
        source_key: match data.settings.contains_key(MASTER_KEY_FINGERPRINT) {
            true => Some(document::SourceKey::Sealed {
                fingerprint: String::new(),
            }),
            false => Some(document::SourceKey::Plaintext),
        },
        data,
    })
}

/// v3 wrote this key exactly when it had a master key, so its presence is how
/// the file says whether its secrets are sealed.
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
    Ok(document::Data {
        organizations: rows(connection, ORGANIZATIONS, organization).await?,
        teams: rows(connection, TEAMS, team).await?,
        users: rows(connection, USERS, user).await?,
        providers: rows(connection, PROVIDERS, provider).await?,
        credentials: rows(connection, CREDENTIALS, credential).await?,
        user_keys: rows(connection, USER_KEYS, user_key).await?,
        quotas: rows(connection, QUOTAS, quota).await?,
        price_rules: rows(connection, PRICE_RULES, price_rule).await?,
        price_rates: rows(connection, PRICE_RATES, price_rate).await?,
        routes: rows(connection, ROUTES, route).await?,
        route_members: rows(connection, ROUTE_MEMBERS, route_member).await?,
        aliases: rows(connection, ALIASES, alias).await?,
        model_aliases: rows(connection, EXPOSED_MODELS, model_alias).await?,
        routing_rules: rows(connection, ROUTING_RULES, routing_rule).await?,
        rule_sets: rows(connection, RULE_SETS, rule_set).await?,
        rules: rows(connection, RULES, rule).await?,
        provider_rule_sets: rows(connection, PROVIDER_RULE_SETS, provider_rule_set).await?,
        // The three v3's own export forgets are ordinary tables in the file,
        // so the direct route has them without any splicing.
        permissions: rows(connection, PERMISSIONS, permission).await?,
        rate_limits: rows(connection, RATE_LIMITS, rate_limit).await?,
        provider_models: rows(connection, PROVIDER_MODELS, provider_model).await?,
        settings: settings(connection).await?,
    })
}

/// Every statement this module runs, in one place so a schema question has one
/// answer. Column order is the argument order of the readers below, and the
/// names are production's `.schema` rather than the `v3` branch's entities:
/// what an operator has is what the live database looks like.
const ORGANIZATIONS: &str = "SELECT id, name, enabled FROM organizations ORDER BY id";
const TEAMS: &str = "SELECT id, organization_id, name, enabled FROM teams ORDER BY id";
const USERS: &str = "SELECT id, name, organization_id, team_id, enabled, is_admin \
                     FROM users ORDER BY id";
const PROVIDERS: &str = "SELECT id, name, channel, settings_json, enabled, label, proxy_url, \
                         credential_strategy FROM providers ORDER BY id";
const CREDENTIALS: &str = "SELECT id, provider_id, label, ciphertext, wrapped_key, \
                           payload_nonce, key_nonce, version, enabled, weight, rpm_limit, \
                           tpm_limit, proxy_url, kind FROM credentials ORDER BY id";
const USER_KEYS: &str = "SELECT id, user_id, digest, digest_version, prefix, label, \
                         expires_at, enabled, ciphertext, wrapped_key, payload_nonce, \
                         key_nonce FROM user_keys ORDER BY id";
const QUOTAS: &str = "SELECT id, subject_kind, subject_id, quota_total, quota_daily, \
                      quota_weekly, quota_monthly, quota_5h, quota_7d, enabled \
                      FROM quotas ORDER BY id";
const PRICE_RULES: &str = "SELECT id, provider_id, model_pattern, tiers_json, priority, \
                           enabled FROM price_rules ORDER BY id";
const PRICE_RATES: &str = "SELECT id, rule_id, metric, unit_size, price, conditions_json, \
                           priority FROM price_rates ORDER BY id";
const ROUTES: &str = "SELECT id, name, strategy, max_attempts, enabled FROM routes ORDER BY id";
const ROUTE_MEMBERS: &str = "SELECT id, route_id, provider_id, upstream_model, tier, weight, \
                             enabled FROM route_members ORDER BY id";
const ALIASES: &str = "SELECT id, alias, target, provider_id, priority, enabled \
                       FROM aliases ORDER BY id";
const EXPOSED_MODELS: &str = "SELECT id, name, route_id, enabled FROM exposed_models ORDER BY id";
const ROUTING_RULES: &str = "SELECT id, provider_id, operation, kind, implementation, \
                             dest_operation, dest_kind, sort_order, enabled, origin \
                             FROM routing_rules ORDER BY id";
const RULE_SETS: &str = "SELECT id, name, description, enabled FROM rule_sets ORDER BY id";
const RULES: &str = "SELECT id, rule_set_id, kind, config_json, filter_model_pattern, \
                     filter_operations_json, filter_header_pattern, sort_order, enabled \
                     FROM rules ORDER BY id";
const PROVIDER_RULE_SETS: &str = "SELECT id, provider_id, rule_set_id, sort_order, enabled \
                                  FROM provider_rule_sets ORDER BY id";
const PERMISSIONS: &str = "SELECT id, subject_kind, subject_id, provider_id, operation_group, \
                           model_pattern, allowed FROM permissions ORDER BY id";
const RATE_LIMITS: &str = "SELECT id, subject_kind, subject_id, requests, window_seconds \
                           FROM rate_limits ORDER BY id";
const PROVIDER_MODELS: &str = "SELECT id, provider_id, model_id, display_name, context_window, \
                               max_output_tokens, enabled FROM provider_models ORDER BY id";
const SETTINGS: &str = "SELECT key, value_json FROM settings ORDER BY key";

/// Run one statement and convert each row. The error names the table, because
/// a v3 database old enough to be missing a column is a real possibility and
/// "no such column: origin" on its own says nothing about what to do.
async fn rows<T>(
    connection: &DatabaseConnection,
    sql: &'static str,
    convert: fn(&QueryResult) -> Result<T>,
) -> Result<Vec<T>> {
    let statement = Statement::from_string(connection.get_database_backend(), sql.to_owned());
    let found = connection.query_all_raw(statement).await.map_err(|error| {
        Error::other(format!(
            "reading the v3 database failed on `{sql}`: {error}. The file may have been \
             written by a v3 older than 3.0.16, whose schema this migration does not read."
        ))
    })?;
    found.iter().map(convert).collect()
}

// ------------------------------------------------------- column readers --

fn integer(row: &QueryResult, index: usize) -> Result<i64> {
    row.try_get_by_index::<i64>(index)
        .map_err(|error| Error::other(format!("v3 column {index} is not an integer: {error}")))
}

fn optional_integer(row: &QueryResult, index: usize) -> Result<Option<i64>> {
    row.try_get_by_index::<Option<i64>>(index)
        .map_err(|error| Error::other(format!("v3 column {index} is not an integer: {error}")))
}

fn text(row: &QueryResult, index: usize) -> Result<String> {
    row.try_get_by_index::<String>(index)
        .map_err(|error| Error::other(format!("v3 column {index} is not text: {error}")))
}

fn optional_text(row: &QueryResult, index: usize) -> Result<Option<String>> {
    row.try_get_by_index::<Option<String>>(index)
        .map_err(|error| Error::other(format!("v3 column {index} is not text: {error}")))
}

fn blob(row: &QueryResult, index: usize) -> Result<Vec<u8>> {
    row.try_get_by_index::<Vec<u8>>(index)
        .map_err(|error| Error::other(format!("v3 column {index} is not a blob: {error}")))
}

fn optional_blob(row: &QueryResult, index: usize) -> Result<Option<Vec<u8>>> {
    row.try_get_by_index::<Option<Vec<u8>>>(index)
        .map_err(|error| Error::other(format!("v3 column {index} is not a blob: {error}")))
}

/// v3 stored every boolean as an integer.
fn flag(row: &QueryResult, index: usize) -> Result<bool> {
    Ok(integer(row, index)? != 0)
}

/// A JSON text column. v3 wrote valid JSON into every one of them, but a
/// hand-edited row is possible and losing the whole migration to one is not
/// worth it: an unparseable value becomes null and the row keeps its shape.
fn json(row: &QueryResult, index: usize) -> Result<Value> {
    Ok(optional_text(row, index)?
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null))
}

/// A decimal kept as text, which is how v3 stored money.
fn decimal(row: &QueryResult, index: usize) -> Result<Option<String>> {
    optional_text(row, index)
}

// --------------------------------------------------------- row readers --

fn organization(row: &QueryResult) -> Result<document::Organization> {
    Ok(document::Organization {
        id: integer(row, 0)?,
        name: text(row, 1)?,
        enabled: flag(row, 2)?,
    })
}

fn team(row: &QueryResult) -> Result<document::Team> {
    Ok(document::Team {
        id: integer(row, 0)?,
        organization_id: integer(row, 1)?,
        name: text(row, 2)?,
        enabled: flag(row, 3)?,
    })
}

fn user(row: &QueryResult) -> Result<document::User> {
    Ok(document::User {
        id: integer(row, 0)?,
        name: text(row, 1)?,
        organization_id: optional_integer(row, 2)?,
        team_id: optional_integer(row, 3)?,
        enabled: flag(row, 4)?,
        is_admin: flag(row, 5)?,
    })
}

fn provider(row: &QueryResult) -> Result<document::Provider> {
    Ok(document::Provider {
        id: integer(row, 0)?,
        name: text(row, 1)?,
        channel: text(row, 2)?,
        settings: json(row, 3)?,
        enabled: flag(row, 4)?,
        label: optional_text(row, 5)?,
        proxy_url: optional_text(row, 6)?,
        credential_strategy: optional_text(row, 7)?,
        tls_fingerprint: None,
        traffic_policy: None,
    })
}

fn credential(row: &QueryResult) -> Result<document::Credential> {
    Ok(document::Credential {
        config: document::CredentialConfig {
            id: integer(row, 0)?,
            provider_id: integer(row, 1)?,
            label: optional_text(row, 2)?,
            version: integer(row, 7)?.max(0) as u64,
            enabled: flag(row, 8)?,
            weight: u32::try_from(integer(row, 9)?).unwrap_or(100),
            rpm_limit: optional_integer(row, 10)?.and_then(|v| u32::try_from(v).ok()),
            tpm_limit: optional_integer(row, 11)?.and_then(|v| u64::try_from(v).ok()),
            proxy_url: optional_text(row, 12)?,
            kind: text(row, 13)?,
        },
        secret: Some(document::Envelope {
            ciphertext: blob(row, 3)?,
            wrapped_key: blob(row, 4)?,
            payload_nonce: blob(row, 5)?,
            key_nonce: blob(row, 6)?,
        }),
    })
}

fn user_key(row: &QueryResult) -> Result<document::UserKey> {
    // Nullable in v3: only a key created as revealable kept its text.
    let ciphertext = optional_blob(row, 8)?;
    let secret = ciphertext.map(|ciphertext| document::Envelope {
        ciphertext,
        wrapped_key: optional_blob(row, 9).ok().flatten().unwrap_or_default(),
        payload_nonce: optional_blob(row, 10).ok().flatten().unwrap_or_default(),
        key_nonce: optional_blob(row, 11).ok().flatten().unwrap_or_default(),
    });
    Ok(document::UserKey {
        config: document::UserKeyConfig {
            id: integer(row, 0)?,
            user_id: integer(row, 1)?,
            prefix: optional_text(row, 4)?,
            label: optional_text(row, 5)?,
            expires_at: optional_integer(row, 6)?,
            enabled: flag(row, 7)?,
        },
        digest: blob(row, 2)?,
        digest_version: u32::try_from(integer(row, 3)?).unwrap_or(0),
        secret,
    })
}

fn quota(row: &QueryResult) -> Result<document::Quota> {
    Ok(document::Quota {
        id: integer(row, 0)?,
        subject_kind: text(row, 1)?,
        subject_id: integer(row, 2)?,
        quota_total: decimal(row, 3)?,
        quota_daily: decimal(row, 4)?,
        quota_weekly: decimal(row, 5)?,
        quota_monthly: decimal(row, 6)?,
        quota_5h: decimal(row, 7)?,
        quota_7d: decimal(row, 8)?,
        enabled: flag(row, 9)?,
    })
}

fn price_rule(row: &QueryResult) -> Result<document::PriceRule> {
    Ok(document::PriceRule {
        id: integer(row, 0)?,
        provider_id: optional_integer(row, 1)?,
        model_pattern: text(row, 2)?,
        tiers: match json(row, 3)? {
            Value::Null => None,
            value => Some(value),
        },
        priority: integer(row, 4)?,
        enabled: flag(row, 5)?,
    })
}

fn price_rate(row: &QueryResult) -> Result<document::PriceRate> {
    Ok(document::PriceRate {
        id: integer(row, 0)?,
        rule_id: integer(row, 1)?,
        metric: text(row, 2)?,
        unit_size: u64::try_from(integer(row, 3)?).unwrap_or(1),
        price: text(row, 4)?,
        conditions: match json(row, 5)? {
            Value::Null => None,
            value => Some(value),
        },
        priority: integer(row, 6)?,
    })
}

fn route(row: &QueryResult) -> Result<document::Route> {
    Ok(document::Route {
        id: integer(row, 0)?,
        name: text(row, 1)?,
        strategy: optional_text(row, 2)?,
        max_attempts: u32::try_from(integer(row, 3)?).unwrap_or(1),
        enabled: flag(row, 4)?,
    })
}

fn route_member(row: &QueryResult) -> Result<document::RouteMember> {
    Ok(document::RouteMember {
        id: integer(row, 0)?,
        route_id: integer(row, 1)?,
        provider_id: integer(row, 2)?,
        upstream_model: text(row, 3)?,
        tier: u32::try_from(integer(row, 4)?).unwrap_or(0),
        weight: u32::try_from(integer(row, 5)?).unwrap_or(100),
        enabled: flag(row, 6)?,
    })
}

fn alias(row: &QueryResult) -> Result<document::Alias> {
    Ok(document::Alias {
        id: integer(row, 0)?,
        alias: text(row, 1)?,
        target: text(row, 2)?,
        provider_id: optional_integer(row, 3)?,
        priority: integer(row, 4)?,
        enabled: flag(row, 5)?,
    })
}

fn model_alias(row: &QueryResult) -> Result<document::ModelAlias> {
    Ok(document::ModelAlias {
        id: integer(row, 0)?,
        name: text(row, 1)?,
        route_id: integer(row, 2)?,
        enabled: flag(row, 3)?,
    })
}

fn routing_rule(row: &QueryResult) -> Result<document::RoutingRule> {
    Ok(document::RoutingRule {
        id: integer(row, 0)?,
        provider_id: integer(row, 1)?,
        operation: text(row, 2)?,
        kind: text(row, 3)?,
        implementation: text(row, 4)?,
        dest_operation: optional_text(row, 5)?,
        dest_kind: optional_text(row, 6)?,
        sort_order: integer(row, 7)?,
        enabled: flag(row, 8)?,
        // The column the export DTO has no field for, and the one that decides
        // whether a row is operator intent or a seeded channel default.
        origin: optional_text(row, 9)?,
    })
}

fn rule_set(row: &QueryResult) -> Result<document::RuleSet> {
    Ok(document::RuleSet {
        id: integer(row, 0)?,
        name: text(row, 1)?,
        description: optional_text(row, 2)?,
        enabled: flag(row, 3)?,
    })
}

/// v3's `rules` row into the nested shape the admin API serves, which is what
/// the rest of the migration reads. The `kind` column and `config_json` are one
/// tagged object there; the locate is `{"type": …, "value": …}` there and
/// `{"path": …}` here.
fn rule(row: &QueryResult) -> Result<document::Rule> {
    let id = integer(row, 0)?;
    let kind = text(row, 2)?;
    let stored = json(row, 3)?;
    let config = rule_config(&kind, &stored).ok_or_else(|| {
        Error::other(format!(
            "v3 rule {id} has kind `{kind}` with a config this migration cannot read. v3 wrote \
             five kinds: system_text, cache_breakpoint, rewrite, transform, header."
        ))
    })?;
    Ok(document::Rule {
        id,
        rule_set_id: integer(row, 1)?,
        config,
        filter_model_pattern: optional_text(row, 4)?,
        filter_operations: match json(row, 5)? {
            Value::Array(items) => Some(
                items
                    .into_iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect(),
            ),
            _ => None,
        },
        filter_header_pattern: optional_text(row, 6)?,
        sort_order: integer(row, 7)?,
        enabled: flag(row, 8)?,
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
        id: integer(row, 0)?,
        provider_id: integer(row, 1)?,
        rule_set_id: integer(row, 2)?,
        sort_order: integer(row, 3)?,
        enabled: flag(row, 4)?,
    })
}

fn permission(row: &QueryResult) -> Result<document::Permission> {
    Ok(document::Permission {
        id: integer(row, 0)?,
        subject_kind: text(row, 1)?,
        subject_id: integer(row, 2)?,
        provider_id: optional_integer(row, 3)?,
        operation_group: optional_text(row, 4)?,
        model_pattern: optional_text(row, 5)?,
        allowed: flag(row, 6)?,
    })
}

fn rate_limit(row: &QueryResult) -> Result<document::RateLimit> {
    Ok(document::RateLimit {
        id: integer(row, 0)?,
        subject_kind: text(row, 1)?,
        subject_id: integer(row, 2)?,
        requests: u64::try_from(integer(row, 3)?).unwrap_or(0),
        window_seconds: u64::try_from(integer(row, 4)?).unwrap_or(60),
    })
}

fn provider_model(row: &QueryResult) -> Result<document::ProviderModel> {
    Ok(document::ProviderModel {
        id: integer(row, 0)?,
        provider_id: integer(row, 1)?,
        model_id: text(row, 2)?,
        display_name: optional_text(row, 3)?,
        context_window: optional_integer(row, 4)?,
        max_output_tokens: optional_integer(row, 5)?,
        metadata: Value::Null,
        enabled: flag(row, 6)?,
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
        out.insert(text(row, 0)?, json(row, 1)?);
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
