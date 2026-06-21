//! MIGRATE-V1 (remove in 2.1): map v1 rows into a v2 `import::Bundle`.
//!
//! The bundle carries PLAINTEXT secrets (v1-decrypted); `import_bundle` re-seals
//! them under the v2 master key. `routing_rules` are NOT mapped here — the v1
//! `routing_json` vocabulary differs from v2's; instead the migration seeds each
//! provider's channel defaults via `api::routing::seed_default_routing` (more
//! correct than translating). Routes/members are synthesized from `models`, the
//! v1 equivalent of v2's route/alias resolution layer.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde_json::{Map, Value};

use super::cipher::V1Cipher;
use super::read::{V1Data, V1Model};
use crate::app::import::{Bundle, CredentialImport, UserKeyImport};
use crate::store::persistence::records::{
    InstanceSettingsInput, OrgInput, ProviderInput, ProviderModelInput, QuotaInput, RateLimitInput,
    RouteInput, RouteMemberInput, RoutePermissionInput, Scope, UserInput,
};

/// v1 had no orgs; all migrated users attach to one synthesized default org.
const DEFAULT_ORG_ID: i64 = 1;
const DEFAULT_ORG_NAME: &str = "default";

/// Build the v2 import bundle from v1 data, decrypting secrets along the way.
pub fn to_bundle(data: &V1Data, cipher: &V1Cipher) -> anyhow::Result<Bundle> {
    let mut bundle = empty_bundle();

    // Default org (v2 users require a non-null org_id; v1 had no orgs/teams).
    bundle.orgs.push(OrgInput {
        id: Some(DEFAULT_ORG_ID),
        name: DEFAULT_ORG_NAME.to_string(),
        enabled: true,
        description: Some("migrated from GPROXY v1".to_string()),
    });

    for u in &data.users {
        bundle.users.push(UserInput {
            id: Some(u.id),
            name: u.name.clone(),
            org_id: DEFAULT_ORG_ID,
            team_id: None,
            password: u.password.clone(),
            enabled: u.enabled,
            is_admin: u.is_admin,
        });
    }

    for k in &data.user_keys {
        // Recover the bare key (plaintext column, or `enc:v2:` envelope).
        let api_key = cipher.decrypt_string(&k.api_key_ciphertext)?;
        bundle.user_keys.push(UserKeyImport {
            id: Some(k.id),
            user_id: k.user_id,
            api_key,
            label: k.label.clone(),
            enabled: k.enabled,
        });
    }

    for p in &data.providers {
        bundle.providers.push(ProviderInput {
            id: Some(p.id),
            name: p.name.clone(),
            channel: p.channel.clone(),
            label: p.label.clone(),
            settings_json: parse_or_empty(&p.settings_json),
            credential_strategy: "round_robin".to_string(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        });
    }

    for c in &data.credentials {
        // Decrypt to plaintext (import_bundle re-seals under the v2 key).
        let secret_json = cipher.decrypt_json(parse_or_empty(&c.secret_json))?;
        bundle.credentials.push(CredentialImport {
            id: Some(c.id),
            provider_id: c.provider_id,
            label: c.name.clone(),
            kind: c.kind.clone(),
            secret_json,
            weight: 100,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
            enabled: c.enabled,
        });
    }

    for m in &data.models {
        bundle.provider_models.push(ProviderModelInput {
            id: Some(m.id),
            provider_id: m.provider_id,
            model_id: m.model_id.clone(),
            display_name: m.display_name.clone(),
            pricing_json: map_pricing(m.pricing_json.as_deref()),
            variants_json: None,
            enabled: m.enabled,
        });
    }

    synth_routes(&data.models, &mut bundle);

    for q in &data.quotas {
        bundle.quotas.push(QuotaInput {
            id: None,
            scope: Scope::User,
            scope_id: q.user_id,
            quota_total: dec(q.quota),
            cost_used: dec(q.cost_used),
        });
    }

    for p in &data.model_perms {
        // route name == model_id, so a v1 model glob carries over as a route glob.
        bundle.route_permissions.push(RoutePermissionInput {
            id: None,
            scope: Scope::User,
            scope_id: p.user_id,
            route_pattern: p.model_pattern.clone(),
        });
    }

    for r in &data.rate_limits {
        bundle.rate_limits.push(RateLimitInput {
            id: None,
            scope: Scope::User,
            scope_id: r.user_id,
            route_pattern: r.model_pattern.clone(),
            rpm: r.rpm,
            rpd: r.rpd,
            total_tokens: r.total_tokens,
        });
    }

    if let Some(s) = &data.settings {
        bundle.instance_settings.push(InstanceSettingsInput {
            id: None,
            instance_name: "default".to_string(),
            proxy: s.proxy.clone(),
            // v1 stored a fingerprint string; v2 is a boolean toggle.
            spoof_emulation: s.spoof_emulation.as_ref().map(|_| true),
            enable_usage: s.enable_usage,
            enable_upstream_log: s.enable_upstream_log,
            enable_upstream_log_body: s.enable_upstream_log_body,
            enable_downstream_log: s.enable_downstream_log,
            enable_downstream_log_body: s.enable_downstream_log_body,
            disable_log_redaction: false,
            enable_tokenizer_download: false,
            update_channel: s.update_channel.clone(),
            retention_days: None,
        });
    }

    Ok(bundle)
}

/// Synthesize one route per unique `model_id` (route name = model id) and one
/// member per v1 model row. Same `model_id` across providers → one route with
/// many members (v2 load-balances what v1 resolved last-write-wins).
fn synth_routes(models: &[V1Model], bundle: &mut Bundle) {
    let mut route_id: BTreeMap<&str, i64> = BTreeMap::new();
    let mut enabled: BTreeMap<i64, bool> = BTreeMap::new();
    let mut order: Vec<(String, i64)> = Vec::new();
    let mut next = 1i64;
    for m in models {
        let name = m.model_id.trim();
        if name.is_empty() {
            continue;
        }
        let rid = *route_id.entry(name).or_insert_with(|| {
            let id = next;
            next += 1;
            order.push((name.to_string(), id));
            id
        });
        *enabled.entry(rid).or_insert(false) |= m.enabled;
        bundle.route_members.push(RouteMemberInput {
            id: None,
            route_id: rid,
            provider_id: m.provider_id,
            upstream_model_id: m.model_id.clone(),
            weight: 100,
            tier: 0,
            enabled: m.enabled,
        });
    }
    for (name, rid) in order {
        bundle.routes.push(RouteInput {
            id: Some(rid),
            name,
            strategy: "failover".to_string(),
            enabled: enabled.get(&rid).copied().unwrap_or(true),
            description: None,
            settings_json: None,
        });
    }
}

/// Collapse v1's tiered `pricing_json` into v2's flat per-million rates. Takes
/// the first (base) tier; v2 has no tiered/flex/scale/priority distinction.
fn map_pricing(v1: Option<&str>) -> Option<Value> {
    let s = v1?;
    if s.trim().is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(s).ok()?;
    let tier = v.get("price_tiers")?.as_array()?.first()?;
    let num = |k: &str| tier.get(k).and_then(Value::as_f64);
    let mut out = Map::new();
    let mut put = |key: &str, val: Option<f64>| {
        if let Some(x) = val {
            out.insert(key.to_string(), Value::String(x.to_string()));
        }
    };
    put("input", num("price_input_tokens"));
    put("output", num("price_output_tokens"));
    put("cache_read", num("price_cache_read_input_tokens"));
    put(
        "cache_creation",
        num("price_cache_creation_input_tokens_5min")
            .or_else(|| num("price_cache_creation_input_tokens_1h"))
            .or_else(|| num("price_cache_creation_input_tokens")),
    );
    if out.is_empty() {
        None
    } else {
        Some(Value::Object(out))
    }
}

fn dec(x: f64) -> Decimal {
    // f64→Decimal is exact, so 0.68182965 becomes 0.6818296500000000648…; round
    // to 12 dp and strip trailing zeros for a clean stored value.
    Decimal::from_f64_retain(x)
        .unwrap_or(Decimal::ZERO)
        .round_dp(12)
        .normalize()
}

fn parse_or_empty(s: &str) -> Value {
    if s.trim().is_empty() {
        return Value::Object(Map::new());
    }
    serde_json::from_str(s).unwrap_or_else(|_| Value::Object(Map::new()))
}

fn empty_bundle() -> Bundle {
    Bundle {
        schema_version: 1,
        orgs: Vec::new(),
        teams: Vec::new(),
        users: Vec::new(),
        user_keys: Vec::new(),
        route_permissions: Vec::new(),
        rate_limits: Vec::new(),
        quotas: Vec::new(),
        providers: Vec::new(),
        credentials: Vec::new(),
        provider_models: Vec::new(),
        routes: Vec::new(),
        route_members: Vec::new(),
        aliases: Vec::new(),
        routing_rules: Vec::new(),
        rule_sets: Vec::new(),
        rules: Vec::new(),
        provider_rule_sets: Vec::new(),
        instance_settings: Vec::new(),
    }
}
