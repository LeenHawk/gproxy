//! Minimal config-bundle import (M9-lite): deserialize a JSON bundle of
//! `*Input` records and upsert in FK order. Idempotent when the bundle pins
//! explicit ids (same file → same rows). Cross-record references (org_id,
//! provider_id, route_id, rule_set_id, user_id) are raw ids — bundles MUST
//! set explicit ids on referenced records.

use serde::{Deserialize, Serialize};

use crate::crypto::SecretCipher;
use crate::pipeline::auth::key_digest;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{
    AliasInput, CredentialInput, InstanceSettingsInput, OrgInput, ProviderInput,
    ProviderModelInput, ProviderRuleSetInput, QuotaInput, RateLimitInput, RouteInput,
    RouteMemberInput, RoutePermissionInput, RoutingRuleInput, RuleInput, RuleSetInput, TeamInput,
    UserInput, UserKeyInput,
};

// No `Debug` on Bundle / UserKeyImport / CredentialImport: they carry bare
// api keys and plaintext secret_json pre-sealing — a derived Debug would let
// one stray log line leak them (§14.1). `Serialize` is for the inverse path
// (`export::export_bundle` → JSON) so `export | import` round-trips.
#[derive(Serialize, Deserialize)]
pub struct Bundle {
    pub schema_version: u32,
    #[serde(default)]
    pub orgs: Vec<OrgInput>,
    #[serde(default)]
    pub teams: Vec<TeamInput>,
    #[serde(default)]
    pub users: Vec<UserInput>,
    #[serde(default)]
    pub user_keys: Vec<UserKeyImport>,
    #[serde(default)]
    pub route_permissions: Vec<RoutePermissionInput>,
    #[serde(default)]
    pub rate_limits: Vec<RateLimitInput>,
    #[serde(default)]
    pub quotas: Vec<QuotaInput>,
    #[serde(default)]
    pub providers: Vec<ProviderInput>,
    #[serde(default)]
    pub credentials: Vec<CredentialImport>,
    #[serde(default)]
    pub provider_models: Vec<ProviderModelInput>,
    #[serde(default)]
    pub routes: Vec<RouteInput>,
    #[serde(default)]
    pub route_members: Vec<RouteMemberInput>,
    #[serde(default)]
    pub aliases: Vec<AliasInput>,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRuleInput>,
    #[serde(default)]
    pub rule_sets: Vec<RuleSetInput>,
    #[serde(default)]
    pub rules: Vec<RuleInput>,
    #[serde(default)]
    pub provider_rule_sets: Vec<ProviderRuleSetInput>,
    #[serde(default)]
    pub instance_settings: Vec<InstanceSettingsInput>,
}

/// `user_keys` import form: bare api key in, digest + ciphertext derived
/// (sealed via the boot cipher; keyless boots store the bare key as before).
#[derive(Serialize, Deserialize)]
pub struct UserKeyImport {
    pub id: Option<i64>,
    pub user_id: i64,
    pub api_key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// `credentials` import form: accepts a `label` field (maps to `name`) and
/// provides defaults for pool fields not needed at dev-bootstrap time.
#[derive(Serialize, Deserialize)]
pub struct CredentialImport {
    pub id: Option<i64>,
    pub provider_id: i64,
    /// Human label — stored in the `name` column.
    #[serde(default)]
    pub label: Option<String>,
    /// Credential kind; defaults to `"api_key"` when omitted.
    #[serde(default = "default_api_key")]
    pub kind: String,
    pub secret_json: serde_json::Value,
    /// Pool weight; defaults to 100.
    #[serde(default = "default_weight")]
    pub weight: i64,
    #[serde(default)]
    pub rpm_limit: Option<i64>,
    #[serde(default)]
    pub tpm_limit: Option<i64>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub tls_fingerprint: Option<serde_json::Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_api_key() -> String {
    "api_key".to_string()
}

fn default_weight() -> i64 {
    100
}

pub struct ImportStats {
    pub records: usize,
}

pub async fn import_bundle(
    db: &dyn PersistenceBackend,
    cipher: &dyn SecretCipher,
    json: &str,
) -> anyhow::Result<ImportStats> {
    let bundle: Bundle = serde_json::from_str(json)?;
    anyhow::ensure!(
        bundle.schema_version == 1,
        "unsupported bundle schema_version {}",
        bundle.schema_version
    );
    let mut n = 0usize;
    for x in bundle.orgs {
        db.upsert_org(x).await?;
        n += 1;
    }
    for x in bundle.teams {
        db.upsert_team(x).await?;
        n += 1;
    }
    for x in bundle.users {
        db.upsert_user(x).await?;
        n += 1;
    }
    for k in bundle.user_keys {
        // The import bundle is the ONLY entrance for external key material
        // (the admin API generates keys server-side): blank keys are rejected;
        // short ones are accepted but flagged — they lack the ≥32-char
        // random-equivalent strength a generated key has.
        let bare = k.api_key.trim();
        if bare.is_empty() {
            anyhow::bail!(
                "user_keys: api_key for user {} must not be blank",
                k.user_id
            );
        }
        if bare.chars().count() < 32 {
            tracing::warn!(
                user_id = k.user_id,
                "imported api_key is shorter than 32 chars — weaker than a generated key"
            );
        }
        // digest from the BARE (trimmed) key (auth lookup), ciphertext sealed
        // (§14.1). NoopCipher seal is identity → the bare string is stored as
        // before; a real cipher yields an envelope object stored as JSON text.
        let digest = key_digest(bare);
        let sealed = cipher.seal(&serde_json::Value::String(bare.to_string()))?;
        let stored = match &sealed {
            serde_json::Value::String(s) => s.clone(),
            other => serde_json::to_string(other)?,
        };
        db.upsert_user_key(UserKeyInput {
            id: k.id,
            user_id: k.user_id,
            api_key_digest: digest,
            api_key_ciphertext: stored,
            label: k.label,
            enabled: k.enabled,
        })
        .await?;
        n += 1;
    }
    for x in bundle.route_permissions {
        db.upsert_route_permission(x).await?;
        n += 1;
    }
    for x in bundle.rate_limits {
        db.upsert_rate_limit(x).await?;
        n += 1;
    }
    for x in bundle.quotas {
        db.upsert_quota(x).await?;
        n += 1;
    }
    for x in bundle.providers {
        db.upsert_provider(x).await?;
        n += 1;
    }
    for c in bundle.credentials {
        db.upsert_credential(CredentialInput {
            id: c.id,
            provider_id: c.provider_id,
            name: c.label,
            kind: c.kind,
            secret_json: cipher.seal(&c.secret_json)?,
            weight: c.weight,
            rpm_limit: c.rpm_limit,
            tpm_limit: c.tpm_limit,
            proxy_url: c.proxy_url,
            tls_fingerprint: c.tls_fingerprint,
            enabled: c.enabled,
        })
        .await?;
        n += 1;
    }
    for x in bundle.provider_models {
        db.upsert_provider_model(x).await?;
        n += 1;
    }
    for x in bundle.routes {
        db.upsert_route(x).await?;
        n += 1;
    }
    for x in bundle.route_members {
        db.upsert_route_member(x).await?;
        n += 1;
    }
    for x in bundle.aliases {
        db.upsert_alias(x).await?;
        n += 1;
    }
    for x in bundle.routing_rules {
        db.upsert_routing_rule(x).await?;
        n += 1;
    }
    for x in bundle.rule_sets {
        db.upsert_rule_set(x).await?;
        n += 1;
    }
    for x in bundle.rules {
        db.upsert_rule(x).await?;
        n += 1;
    }
    for x in bundle.provider_rule_sets {
        db.upsert_provider_rule_set(x).await?;
        n += 1;
    }
    for x in bundle.instance_settings {
        db.upsert_instance_settings(x).await?;
        n += 1;
    }
    Ok(ImportStats { records: n })
}
