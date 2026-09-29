//! Read v3 rows through typed projections on native databases, D1 and libSQL.
use super::{
    Error, Result,
    document::{self, Document},
};
use gproxy_seaorm::BatchConnectionTrait;
use sea_orm::QueryResult;
use serde_json::{Value, json};
use std::collections::BTreeMap;
mod connection;
pub use connection::Source;
pub async fn read<C: BatchConnectionTrait>(source: &Source<'_, C>) -> Result<Document> {
    let data = data(source).await?;
    Ok(Document {
        format_version: document::FORMAT_VERSION,
        secrets: document::Secrets::Included,
        source_key: Some(source_key(&data)),
        data,
    })
}
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

async fn data<C: BatchConnectionTrait>(connection: &Source<'_, C>) -> Result<document::Data> {
    let tables = &connection.tables;
    Ok(document::Data {
        organizations: rows(connection, tables, "organizations", organization).await?,
        teams: rows(connection, tables, "teams", team).await?,
        users: rows(connection, tables, "users", user).await?,
        providers: rows(connection, tables, "providers", provider).await?,
        credentials: rows(connection, tables, "credentials", credential).await?,
        user_keys: rows(connection, tables, "user_keys", user_key).await?,
        quotas: rows(connection, tables, "quotas", quota).await?,
        price_rules: rows(connection, tables, "price_rules", price_rule).await?,
        price_rates: rows(connection, tables, "price_rates", price_rate).await?,
        routes: rows(connection, tables, "routes", route).await?,
        route_members: rows(connection, tables, "route_members", route_member).await?,
        aliases: rows(connection, tables, "aliases", alias).await?,
        model_aliases: rows(connection, tables, "exposed_models", model_alias).await?,
        routing_rules: rows(connection, tables, "routing_rules", routing_rule).await?,
        rule_sets: rows(connection, tables, "rule_sets", rule_set).await?,
        rules: rows(connection, tables, "rules", rule).await?,
        provider_rule_sets: rows(connection, tables, "provider_rule_sets", provider_rule_set)
            .await?,
        // The three v3's own export forgets are ordinary tables in the file,
        // so the direct route has them without any splicing.
        permissions: rows(connection, tables, "permissions", permission).await?,
        rate_limits: rows(connection, tables, "rate_limits", rate_limit).await?,
        provider_models: {
            let mut models = rows(connection, tables, "provider_models", provider_model).await?;
            capabilities(connection, tables, &mut models).await?;
            models
        },
        oauth_clients: rows(connection, tables, "oauth_clients", oauth_client).await?,
        tokenizer_vocabs: rows(connection, tables, "tokenizer_vocabs", tokenizer_vocab).await?,
        tokenizer_auth: tokenizer_auth(connection, tables).await?,
        settings: settings(connection).await?,
    })
}

fn tokenizer_vocab(row: &QueryResult) -> Result<document::TokenizerVocab> {
    Ok(document::TokenizerVocab {
        name: text(row, "name")?,
        bytes: blob(row, "bytes")?,
    })
}

/// v3 kept one row per token kind; the Hugging Face one is the only kind it
/// ever wrote (`v3:crates/gproxy-app/src/host/tokenizers.rs`).
async fn tokenizer_auth<C: BatchConnectionTrait>(
    connection: &Source<'_, C>,
    tables: &[String],
) -> Result<Option<document::Envelope>> {
    if !tables.iter().any(|name| name == "tokenizer_auth") {
        return Ok(None);
    }

    let Some(row) = connection
        .rows("tokenizer_auth", &[], None, None)
        .await?
        .into_iter()
        .find(|row| optional_text(row, "kind").as_deref() == Some("hugging_face"))
    else {
        return Ok(None);
    };
    Ok(Some(document::Envelope {
        ciphertext: blob(&row, "ciphertext")?,
        wrapped_key: blob(&row, "wrapped_key")?,
        payload_nonce: blob(&row, "payload_nonce")?,
        key_nonce: blob(&row, "key_nonce")?,
    }))
}

fn oauth_client(row: &QueryResult) -> Result<document::OAuthClient> {
    Ok(document::OAuthClient {
        client_id: text(row, "client_id")?,
        name: text(row, "name")?,
        redirect_uris: optional_text(row, "redirect_uris")
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default(),
        enabled: flag(row, "enabled", true),
        retired: optional_text(row, "deleted_at").is_some()
            || optional_integer(row, "deleted_at").is_some(),
    })
}

///
/// A table that is not there at all is empty rather than fatal: v3 created
/// every one of these at schema version 1, but a hand-trimmed database is a
/// thing operators produce, and refusing to migrate the other nineteen tables
/// because of it would help nobody.
async fn rows<T, C: BatchConnectionTrait>(
    connection: &Source<'_, C>,
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
    let order = match table {
        "oauth_clients" => "client_id",
        "tokenizer_vocabs" => "name",
        _ => "id",
    };
    let found = connection.rows(table, &[order], None, None).await?;
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
        tls_fingerprint: Some(json(row, "tls_fingerprint")).filter(|value| !value.is_null()),
        // Lives inside `settings_json`, where `provider_config` reads it.
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
            tls_fingerprint: Some(json(row, "tls_fingerprint")).filter(|value| !value.is_null()),
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
        context_window: optional_integer(row, "context_window"),
        max_output_tokens: optional_integer(row, "max_output_tokens"),
        thinking_supported: optional_flag(row, "thinking_supported"),
        thinking_adaptive_supported: optional_flag(row, "thinking_adaptive_supported"),
        thinking_enabled_supported: optional_flag(row, "thinking_enabled_supported"),
        metadata: model_metadata(row),
        variants: json(row, "variants_json"),
        enabled: flag(row, "enabled", true),
    })
}

/// A nullable boolean column: v3 left a capability NULL when it was unknown.
fn optional_flag(row: &QueryResult, column: &str) -> Option<bool> {
    optional_integer(row, column).map(|value| value != 0)
}

/// v3 spread a model's capabilities over forty columns; its export folded
/// them into one `metadata` object, and this builds the same object from the
/// columns, under the export's names, so both routes produce one shape.
fn model_metadata(row: &QueryResult) -> Value {
    let mut out = serde_json::Map::new();
    for key in [
        "description",
        "instructions",
        "default_reasoning_level",
        "default_service_tier",
        "shell_type",
        "default_verbosity",
        "default_reasoning_summary",
        "apply_patch_tool_type",
        "web_search_tool_type",
        "truncation_mode",
    ] {
        if let Some(value) = optional_text(row, key).filter(|value| !value.is_empty()) {
            out.insert(key.into(), Value::from(value));
        }
    }
    for key in [
        "max_context_window",
        "truncation_limit",
        "auto_compact_token_limit",
        "effective_context_window_percent",
    ] {
        if let Some(value) = optional_integer(row, key) {
            out.insert(key.into(), Value::from(value));
        }
    }
    for (column, key) in [
        ("support_verbosity", "support_verbosity"),
        (
            "reasoning_summary_supported",
            "supports_reasoning_summary_parameter",
        ),
        ("batch_supported", "batch_supported"),
        ("citations_supported", "citations_supported"),
        ("code_execution_supported", "code_execution_supported"),
        (
            "context_management_supported",
            "context_management_supported",
        ),
        (
            "structured_outputs_supported",
            "structured_outputs_supported",
        ),
        ("pdf_input_supported", "pdf_input_supported"),
        (
            "image_detail_original_supported",
            "supports_image_detail_original",
        ),
        ("search_supported", "supports_search_tool"),
    ] {
        if let Some(value) = optional_flag(row, column) {
            out.insert(key.into(), Value::Bool(value));
        }
    }
    // Which side-table lists v3 knew, so an empty one reads as "none" rather
    // than "unknown"; `capabilities` fills them in.
    for (column, key) in [
        ("input_modalities_known", "input_modalities"),
        ("output_modalities_known", "output_modalities"),
        ("parameters_known", "supported_parameters"),
        ("reasoning_levels_known", "reasoning_levels"),
        ("service_tiers_known", "service_tiers"),
        ("generation_methods_known", "generation_methods"),
        ("supported_actions_known", "supported_actions"),
    ] {
        if optional_flag(row, column) == Some(true) {
            out.insert(key.into(), Value::Array(Vec::new()));
        }
    }
    Value::Object(out)
}

/// v3's five per-model side tables, keyed by `(provider_id, model_id)`, as
/// the lists the export put in `metadata`.
async fn capabilities<C: BatchConnectionTrait>(
    connection: &Source<'_, C>,
    tables: &[String],
    models: &mut [document::ProviderModel],
) -> Result<()> {
    type Entry = (
        &'static str,
        fn(&QueryResult) -> Option<(&'static str, Value)>,
    );
    const SIDES: [Entry; 5] = [
        ("provider_model_modalities", |row| {
            let key = match optional_text(row, "direction")?.as_str() {
                "input" => "input_modalities",
                "output" => "output_modalities",
                _ => return None,
            };
            Some((key, Value::from(optional_text(row, "modality")?)))
        }),
        ("provider_model_parameters", |row| {
            Some((
                "supported_parameters",
                Value::from(optional_text(row, "parameter")?),
            ))
        }),
        ("provider_model_reasoning_levels", |row| {
            Some((
                "reasoning_levels",
                serde_json::json!({
                    "effort": optional_text(row, "effort")?,
                    "description": optional_text(row, "description").unwrap_or_default(),
                }),
            ))
        }),
        ("provider_model_service_tiers", |row| {
            Some((
                "service_tiers",
                serde_json::json!({
                    "id": optional_text(row, "tier_id")?,
                    "name": optional_text(row, "name").unwrap_or_default(),
                    "description": optional_text(row, "description").unwrap_or_default(),
                }),
            ))
        }),
        ("provider_model_methods", |row| {
            let key = match optional_text(row, "kind")?.as_str() {
                "generation" => "generation_methods",
                "action" => "supported_actions",
                _ => return None,
            };
            Some((key, Value::from(optional_text(row, "method")?)))
        }),
    ];
    for (table, convert) in SIDES {
        if !tables.iter().any(|name| name == table) {
            continue;
        }
        for row in connection
            .rows(
                table,
                &["provider_id", "model_id", "sort_order"],
                None,
                None,
            )
            .await?
        {
            let (Some(provider_id), Some(model_id)) = (
                optional_integer(&row, "provider_id"),
                optional_text(&row, "model_id"),
            ) else {
                continue;
            };
            let Some((key, value)) = convert(&row) else {
                continue;
            };
            let Some(model) = models
                .iter_mut()
                .find(|model| model.provider_id == provider_id && model.model_id == model_id)
            else {
                continue;
            };
            if let Some(list) = model
                .metadata
                .as_object_mut()
                .map(|map| map.entry(key).or_insert_with(|| Value::Array(Vec::new())))
                .and_then(Value::as_array_mut)
            {
                list.push(value);
            }
        }
    }
    Ok(())
}

/// v3's key/value settings table. The export omits it entirely, so this is the
/// only route that carries any of it.
async fn settings<C: BatchConnectionTrait>(
    connection: &Source<'_, C>,
) -> Result<BTreeMap<String, Value>> {
    let found = connection
        .rows("settings", &["key"], None, None)
        .await
        .map_err(|error| Error::other(format!("reading v3 settings: {error}")))?;
    let mut out = BTreeMap::new();
    for row in &found {
        out.insert(text(row, "key")?, json(row, "value_json"));
    }
    Ok(out)
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
}
