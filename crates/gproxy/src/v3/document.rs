//! The v3 export document, as v3 writes it.
//!
//! Every type here mirrors a DTO in v3's `crates/gproxy-admin/src/dto`, field
//! for field and spelling for spelling, so a document v3 produced deserializes
//! without a translation step. v3's DTOs carry no `rename_all`, so the wire
//! names are Rust's `snake_case`; ids are `i64`.
//!
//! Only what the migration reads is declared. Everything else in the document —
//! a credential's health, a provider's TLS fingerprint diagnostics, a rule's
//! `inherited` flag — is ignored rather than rejected, and every list defaults
//! to empty so a document written by an older v3 still loads.
//!
//! # The three lists v3's export forgot
//!
//! [`Data::permissions`], [`Data::rate_limits`] and [`Data::provider_models`]
//! are **not** produced by `POST /admin/api/export`; v3's
//! `handlers/transfer/export.rs` simply does not collect them, though all three
//! are real v3 tables with real v3 list endpoints. They are optional here and
//! carry v3's own list DTOs verbatim, so an operator can fill them from
//! `GET /admin/api/permissions`, `GET /admin/api/rate-limits` and
//! `GET /admin/api/provider-models` without any reshaping. A document without
//! them imports; it simply arrives without those rows.

use serde::Deserialize;
use serde_json::Value;

/// The only format version v3 ever wrote.
pub const FORMAT_VERSION: u32 = 1;

/// The only user-key digest rule v3 ever implemented: `SHA-256` of the key with
/// an `sk-`/`at-` presentation prefix removed. See
/// `v3:crates/gproxy-app/src/control/user_key.rs`.
pub const DIGEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
pub struct Document {
    pub format_version: u32,
    #[serde(default)]
    pub secrets: Secrets,
    /// How the source sealed its secrets, when it said. `Plaintext` means the
    /// instance ran without `GPROXY_MASTER_KEY`.
    #[serde(default)]
    pub source_key: Option<SourceKey>,
    pub data: Data,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Secrets {
    #[default]
    Omitted,
    Included,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SourceKey {
    Plaintext,
    Sealed {
        #[serde(default)]
        fingerprint: String,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub organizations: Vec<Organization>,
    #[serde(default)]
    pub teams: Vec<Team>,
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub credentials: Vec<Credential>,
    #[serde(default)]
    pub user_keys: Vec<UserKey>,
    #[serde(default)]
    pub quotas: Vec<Quota>,
    #[serde(default)]
    pub price_rules: Vec<PriceRule>,
    #[serde(default)]
    pub price_rates: Vec<PriceRate>,
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub route_members: Vec<RouteMember>,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    #[serde(default)]
    pub model_aliases: Vec<ModelAlias>,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRule>,
    #[serde(default)]
    pub rule_sets: Vec<RuleSet>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub provider_rule_sets: Vec<ProviderRuleSet>,

    // The three below are supplements; see the module note.
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub rate_limits: Vec<RateLimit>,
    #[serde(default)]
    pub provider_models: Vec<ProviderModel>,

    /// v3's key/value `settings` table. The admin API's export carries none of
    /// it, so this is empty on that route and filled on the file one.
    #[serde(default)]
    pub settings: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Organization {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub id: i64,
    pub organization_id: i64,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    #[serde(default)]
    pub password_hash: Option<String>,
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub organization_id: Option<i64>,
    #[serde(default)]
    pub team_id: Option<i64>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub is_admin: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Provider {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    pub channel: String,
    /// v3's `settings_json`, whatever the channel put there.
    #[serde(default)]
    pub settings: Value,
    #[serde(default)]
    pub credential_strategy: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    #[serde(default)]
    pub traffic_policy: Option<Value>,
    #[serde(default)]
    pub enabled: bool,
}

/// v3 flattened nothing: the row is under `config` and the sealed bytes beside
/// it.
#[derive(Debug, Clone, Deserialize)]
pub struct Credential {
    pub config: CredentialConfig,
    #[serde(default)]
    pub secret: Option<Envelope>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CredentialConfig {
    pub id: i64,
    pub provider_id: i64,
    #[serde(default)]
    pub label: Option<String>,
    /// v3's `kind`; v4 calls the same column `auth_kind`.
    #[serde(default = "api_key_kind")]
    pub kind: String,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weight: u32,
    #[serde(default)]
    pub rpm_limit: Option<u32>,
    #[serde(default)]
    pub tpm_limit: Option<u64>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    /// A TLS / HTTP/2 / header fingerprint, the same object a provider carries.
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
}

fn api_key_kind() -> String {
    "api_key".to_owned()
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserKey {
    pub config: UserKeyConfig,
    /// The stored digest, byte for byte. This is the field the whole migration
    /// exists for: carried across, an operator's key keeps working.
    pub digest: Vec<u8>,
    pub digest_version: u32,
    /// The sealed key text, present only for a key v3 was told to keep
    /// revealable *and* an export taken with secrets.
    #[serde(default)]
    pub secret: Option<Envelope>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserKeyConfig {
    pub id: i64,
    pub user_id: i64,
    /// v3's presentation prefix — `sk-` or `at-` — not v4's display prefix.
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub enabled: bool,
}

/// v3's envelope encryption: a per-row data key encrypts the JSON, the master
/// key wraps the data key. All four fields are empty when the source ran
/// without a master key, in which case `ciphertext` is the plaintext JSON.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Envelope {
    #[serde(default)]
    pub ciphertext: Vec<u8>,
    #[serde(default)]
    pub wrapped_key: Vec<u8>,
    #[serde(default)]
    pub payload_nonce: Vec<u8>,
    #[serde(default)]
    pub key_nonce: Vec<u8>,
}

impl Envelope {
    /// Whether this envelope is the unsealed shape: bytes, and no key material.
    pub fn is_plaintext(&self) -> bool {
        self.wrapped_key.is_empty() && self.payload_nonce.is_empty() && self.key_nonce.is_empty()
    }
}

/// One v3 quota row: up to six limits on one subject, one column each.
#[derive(Debug, Clone, Deserialize)]
pub struct Quota {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    #[serde(default)]
    pub quota_total: Option<String>,
    #[serde(default)]
    pub quota_daily: Option<String>,
    #[serde(default)]
    pub quota_weekly: Option<String>,
    #[serde(default)]
    pub quota_monthly: Option<String>,
    #[serde(default, rename = "quota_5h")]
    pub quota_5h: Option<String>,
    #[serde(default, rename = "quota_7d")]
    pub quota_7d: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriceRule {
    pub id: i64,
    #[serde(default)]
    pub provider_id: Option<i64>,
    pub model_pattern: String,
    /// v3's per-rule service-tier multipliers, as one opaque JSON object.
    #[serde(default)]
    pub tiers: Option<Value>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriceRate {
    pub id: i64,
    pub rule_id: i64,
    pub metric: String,
    #[serde(default)]
    pub unit_size: u64,
    pub price: String,
    #[serde(default)]
    pub conditions: Option<Value>,
    #[serde(default)]
    pub priority: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub max_attempts: u32,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteMember {
    pub id: i64,
    pub route_id: i64,
    pub provider_id: i64,
    pub upstream_model: String,
    #[serde(default)]
    pub tier: u32,
    #[serde(default)]
    pub weight: u32,
    #[serde(default)]
    pub enabled: bool,
}

/// A v3 model alias: "when a request says `alias`, send `target`", optionally
/// only for one provider. v4 has no such table.
#[derive(Debug, Clone, Deserialize)]
pub struct Alias {
    pub id: i64,
    pub alias: String,
    pub target: String,
    #[serde(default)]
    pub provider_id: Option<i64>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub enabled: bool,
}

/// v3's `exposed_models`, exported under the name the console used.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelAlias {
    pub id: i64,
    pub name: String,
    pub route_id: i64,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoutingRule {
    pub id: i64,
    pub provider_id: i64,
    pub operation: String,
    /// The incoming dialect this rule is about.
    pub kind: String,
    pub implementation: String,
    #[serde(default)]
    pub dest_operation: Option<String>,
    #[serde(default)]
    pub dest_kind: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub enabled: bool,
    /// `operator` or `channel_default`. A column the admin API's export has no
    /// field for, and the one that says whether a row is somebody's decision or
    /// a copy of what v3's channel already declared — every row in the local v3
    /// database is `channel_default`. Absent on the export route.
    #[serde(default)]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleSet {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: i64,
    pub rule_set_id: i64,
    pub config: RuleConfig,
    #[serde(default)]
    pub filter_model_pattern: Option<String>,
    #[serde(default)]
    pub filter_operations: Option<Vec<String>>,
    #[serde(default)]
    pub filter_header_pattern: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub enabled: bool,
}

/// v3's five rule kinds. Only `Transform` has a v4 counterpart; the rest are
/// kept as data so the report can name what did not travel and why.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleConfig {
    SystemText {
        text: String,
        position: String,
    },
    CacheBreakpoint {
        target: String,
        #[serde(default)]
        index: Option<i64>,
        #[serde(default)]
        ttl: Option<String>,
    },
    Rewrite {
        path: String,
        action: String,
        #[serde(default)]
        value: Option<Value>,
    },
    Transform {
        phase: String,
        locate: Locate,
        actions: Vec<Action>,
        #[serde(default)]
        limit: Option<usize>,
    },
    Header {
        name: String,
        value: String,
        mode: String,
    },
}

impl RuleConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SystemText { .. } => "system_text",
            Self::CacheBreakpoint { .. } => "cache_breakpoint",
            Self::Rewrite { .. } => "rewrite",
            Self::Transform { .. } => "transform",
            Self::Header { .. } => "header",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Locate {
    Path(String),
    Paths(Vec<String>),
    Match(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Action {
    ReplaceText {
        #[serde(default)]
        from: Option<String>,
        with: String,
    },
    ReplaceRegex {
        pattern: String,
        with: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderRuleSet {
    pub id: i64,
    pub provider_id: i64,
    pub rule_set_id: i64,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Permission {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    #[serde(default)]
    pub provider_id: Option<i64>,
    #[serde(default)]
    pub operation_group: Option<String>,
    #[serde(default)]
    pub model_pattern: Option<String>,
    #[serde(default)]
    pub allowed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimit {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub requests: u64,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderModel {
    pub id: i64,
    pub provider_id: i64,
    /// v3's upstream name for the model, which v4 calls `upstream_name`.
    pub model_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
    #[serde(default)]
    pub metadata: Value,
    /// Variant names served off this model: a bare array, or
    /// `{variants, expose_base}` (v3 `records/model.rs::parse_model_variants`).
    #[serde(default)]
    pub variants: Value,
    #[serde(default)]
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_with_only_its_envelope_loads() {
        let export: Document = serde_json::from_str(
            r#"{"format_version":1,"secrets":"omitted","source_key":null,"data":{}}"#,
        )
        .unwrap();
        assert_eq!(export.format_version, FORMAT_VERSION);
        assert_eq!(export.secrets, Secrets::Omitted);
        assert!(export.data.providers.is_empty());
    }

    #[test]
    fn a_sealed_source_key_carries_its_fingerprint() {
        let export: Document = serde_json::from_str(
            r#"{"format_version":1,"secrets":"included",
                "source_key":{"mode":"sealed","fingerprint":"abcd"},"data":{}}"#,
        )
        .unwrap();
        assert!(matches!(export.source_key, Some(SourceKey::Sealed { .. })));
    }

    #[test]
    fn a_credential_keeps_v3s_nesting_and_a_digest_is_a_byte_array() {
        let export: Document = serde_json::from_str(
            r#"{"format_version":1,"data":{
                 "credentials":[{"config":{"id":7,"provider_id":2,"kind":"oauth"},
                                 "secret":{"ciphertext":[1,2],"wrapped_key":[],
                                           "payload_nonce":[],"key_nonce":[]}}],
                 "user_keys":[{"config":{"id":3,"user_id":1,"prefix":"sk-"},
                               "digest":[0,255],"digest_version":1}]}}"#,
        )
        .unwrap();
        assert_eq!(export.data.credentials[0].config.id, 7);
        assert_eq!(export.data.credentials[0].config.kind, "oauth");
        assert!(
            export.data.credentials[0]
                .secret
                .as_ref()
                .unwrap()
                .is_plaintext()
        );
        assert_eq!(export.data.user_keys[0].digest, vec![0, 255]);
        assert_eq!(export.data.user_keys[0].digest_version, DIGEST_VERSION);
    }

    #[test]
    fn the_five_rule_kinds_round_trip_by_their_tag() {
        let rules: Vec<Rule> = serde_json::from_str(
            r#"[{"id":1,"rule_set_id":1,"config":{"kind":"system_text","text":"x","position":"append"}},
                {"id":2,"rule_set_id":1,"config":{"kind":"transform","phase":"request",
                  "locate":{"type":"paths","value":["a.b"]},
                  "actions":[{"op":"replace_regex","pattern":"x","with":"y"}]}}]"#,
        )
        .unwrap();
        assert_eq!(rules[0].config.kind(), "system_text");
        assert_eq!(rules[1].config.kind(), "transform");
    }

    /// The three lists v3's own export omits are optional, so a stock document
    /// loads and a supplemented one carries them.
    #[test]
    fn the_supplementary_lists_default_to_empty_and_read_v3s_own_dtos() {
        let bare: Document = serde_json::from_str(r#"{"format_version":1,"data":{}}"#).unwrap();
        assert!(bare.data.permissions.is_empty());

        let full: Document = serde_json::from_str(
            r#"{"format_version":1,"data":{
                 "permissions":[{"id":1,"subject_kind":"user_key","subject_id":3,
                                 "provider_id":null,"operation_group":null,
                                 "model_pattern":"gpt-*","allowed":true}],
                 "rate_limits":[{"id":2,"subject_kind":"user","subject_id":1,
                                 "requests":60,"window_seconds":60}]}}"#,
        )
        .unwrap();
        assert_eq!(full.data.permissions[0].subject_kind, "user_key");
        assert_eq!(full.data.rate_limits[0].requests, 60);
    }
}
