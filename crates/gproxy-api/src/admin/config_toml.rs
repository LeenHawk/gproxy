use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};

use gproxy_server::AppState;

use crate::auth::authorize_admin;
use crate::error::HttpError;

// ---------------------------------------------------------------------------
// TOML schema
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct GproxyToml {
    #[serde(default)]
    pub global: Option<GlobalSettingsToml>,
    #[serde(default)]
    pub providers: Vec<ProviderToml>,
    #[serde(default)]
    pub models: Vec<ModelToml>,
    #[serde(default)]
    pub model_aliases: Vec<ModelAliasToml>,
    #[serde(default)]
    pub users: Vec<UserToml>,
    #[serde(default)]
    pub permissions: Vec<PermissionToml>,
    #[serde(default)]
    pub file_permissions: Vec<FilePermissionToml>,
    #[serde(default)]
    pub rate_limits: Vec<RateLimitToml>,
    #[serde(default)]
    pub quotas: Vec<QuotaToml>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalSettingsToml {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default = "default_spoof")]
    pub spoof_emulation: String,
    #[serde(default = "default_update_source")]
    pub update_source: String,
    #[serde(default = "default_true")]
    pub enable_usage: bool,
    #[serde(default = "default_false")]
    pub enable_upstream_log: bool,
    #[serde(default = "default_false")]
    pub enable_upstream_log_body: bool,
    #[serde(default = "default_false")]
    pub enable_downstream_log: bool,
    #[serde(default = "default_false")]
    pub enable_downstream_log_body: bool,
    pub dsn: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

fn default_spoof() -> String {
    "chrome_136".to_string()
}
fn default_update_source() -> String {
    "github".to_string()
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_data_dir() -> String {
    "./data".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderToml {
    pub name: String,
    pub channel: String,
    #[serde(default)]
    pub settings: serde_json::Value,
    #[serde(default)]
    pub credentials: Vec<serde_json::Value>,
}

/// Partial representation of a model row for TOML import/export.
///
/// **Lossy roundtrip warning:** this shape only carries default-mode pricing
/// (`price_each_call` + `price_tiers`). Mode variants
/// (`flex_*`, `scale_*`, `priority_*`) and `tool_call_prices` live in
/// `models.pricing_json` in the DB and are NOT exported to TOML, nor can they
/// be configured via TOML import. Use the admin HTTP API / console to manage
/// those. See `docs/superpowers/plans/2026-04-12-pricing-and-tool-billing-fix.md`
/// "Out of Scope" for the follow-up that will extend TOML to cover the full
/// `ModelPrice` shape.
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelToml {
    pub provider_name: String,
    pub model_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub price_each_call: Option<f64>,
    #[serde(default)]
    pub price_tiers: Vec<gproxy_sdk::provider::billing::ModelPriceTier>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelAliasToml {
    pub alias: String,
    pub provider_name: String,
    pub model_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserKeyToml {
    pub api_key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserToml {
    pub name: String,
    /// Plaintext password or an Argon2 PHC hash.
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_false")]
    pub is_admin: bool,
    #[serde(default)]
    pub keys: Vec<UserKeyToml>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PermissionToml {
    pub user_name: String,
    #[serde(default)]
    pub provider_name: Option<String>,
    pub model_pattern: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilePermissionToml {
    pub user_name: String,
    pub provider_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RateLimitToml {
    pub user_name: String,
    pub model_pattern: String,
    #[serde(default)]
    pub rpm: Option<i32>,
    #[serde(default)]
    pub rpd: Option<i32>,
    #[serde(default)]
    pub total_tokens: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuotaToml {
    pub user_name: String,
    pub quota: f64,
    #[serde(default)]
    pub cost_used: f64,
}

// ---------------------------------------------------------------------------
// Export: memory → TOML
// ---------------------------------------------------------------------------

pub async fn export_toml(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<String, HttpError> {
    authorize_admin(&headers, &state)?;

    let config = state.config();
    let engine = state.engine();
    let store = engine.store();

    // Global settings
    let global = GlobalSettingsToml {
        host: config.host.clone(),
        port: config.port,
        proxy: config.proxy.clone(),
        spoof_emulation: config.spoof_emulation.clone(),
        update_source: config.update_source.clone(),
        enable_usage: config.enable_usage,
        enable_upstream_log: config.enable_upstream_log,
        enable_upstream_log_body: config.enable_upstream_log_body,
        enable_downstream_log: config.enable_downstream_log,
        enable_downstream_log_body: config.enable_downstream_log_body,
        dsn: config.dsn.clone(),
        data_dir: config.data_dir.clone(),
    };

    // Providers + credentials from SDK store
    let provider_snapshots = store
        .list_providers()
        .map_err(|e| HttpError::internal(e.to_string()))?;
    let mut providers = Vec::new();
    for p in &provider_snapshots {
        let creds = store
            .list_credentials(Some(&p.name))
            .map_err(|e| HttpError::internal(e.to_string()))?;
        providers.push(ProviderToml {
            name: p.name.clone(),
            channel: p.channel.clone(),
            settings: p.settings.clone(),
            credentials: creds.into_iter().map(|c| c.credential).collect(),
        });
    }

    // Models
    let memory_models = state.models();
    // Build provider_id → provider_name map from SDK engine snapshots
    let provider_id_to_name: std::collections::HashMap<i64, String> = {
        let db_providers = state
            .storage()
            .list_providers(&gproxy_storage::ProviderQuery::default())
            .await
            .unwrap_or_default();
        db_providers.into_iter().map(|p| (p.id, p.name)).collect()
    };
    let models: Vec<ModelToml> = memory_models
        .iter()
        .filter(|m| m.alias_of.is_none())
        .map(|m| ModelToml {
            provider_name: provider_id_to_name
                .get(&m.provider_id)
                .cloned()
                .unwrap_or_else(|| m.provider_id.to_string()),
            model_id: m.model_id.clone(),
            display_name: m.display_name.clone(),
            enabled: m.enabled,
            price_each_call: m.pricing.as_ref().and_then(|p| p.price_each_call),
            price_tiers: m
                .pricing
                .as_ref()
                .map(|p| p.price_tiers.clone())
                .unwrap_or_default(),
        })
        .collect();

    // Model aliases — derived from models with alias_of set
    let model_aliases: Vec<ModelAliasToml> = memory_models
        .iter()
        .filter(|m| m.alias_of.is_some())
        .filter_map(|m| {
            let target_id = m.alias_of?;
            let target = memory_models.iter().find(|t| t.id == target_id)?;
            Some(ModelAliasToml {
                alias: m.model_id.clone(),
                provider_name: provider_id_to_name
                    .get(&target.provider_id)
                    .cloned()
                    .unwrap_or_else(|| target.provider_id.to_string()),
                model_id: target.model_id.clone(),
                enabled: m.enabled,
            })
        })
        .collect();

    // Users + keys
    let users_snapshot = state.users_snapshot();
    let keys_snapshot = state.keys_snapshot();
    let users: Vec<UserToml> = users_snapshot
        .iter()
        .map(|u| {
            let mut user_keys: Vec<_> = keys_snapshot
                .values()
                .filter(|k| k.user_id == u.id)
                .cloned()
                .collect();
            user_keys.sort_by_key(|k| k.id);
            UserToml {
                name: u.name.clone(),
                password: u.password_hash.clone(),
                enabled: u.enabled,
                is_admin: u.is_admin,
                keys: user_keys
                    .into_iter()
                    .map(|k| UserKeyToml {
                        api_key: k.api_key,
                        label: k.label,
                        enabled: k.enabled,
                    })
                    .collect(),
            }
        })
        .collect();

    // Permissions
    let perms_snapshot = state.user_permissions_snapshot();
    let user_name_map: std::collections::HashMap<i64, String> = users_snapshot
        .iter()
        .map(|u| (u.id, u.name.clone()))
        .collect();
    let mut permissions = Vec::new();
    for (user_id, entries) in perms_snapshot.iter() {
        let user_name = user_name_map.get(user_id).cloned().unwrap_or_default();
        for e in entries {
            permissions.push(PermissionToml {
                user_name: user_name.clone(),
                provider_name: e.provider_id.map(|id| {
                    provider_id_to_name
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| id.to_string())
                }),
                model_pattern: e.model_pattern.clone(),
            });
        }
    }

    // File permissions
    let file_perms_snapshot = state.user_file_permissions_snapshot();
    let mut file_permissions = Vec::new();
    for (user_id, entries) in file_perms_snapshot.iter() {
        let user_name = user_name_map.get(user_id).cloned().unwrap_or_default();
        for entry in entries {
            file_permissions.push(FilePermissionToml {
                user_name: user_name.clone(),
                provider_name: provider_id_to_name
                    .get(&entry.provider_id)
                    .cloned()
                    .unwrap_or_else(|| entry.provider_id.to_string()),
            });
        }
    }

    // Rate limits
    let limits_snapshot = state.user_rate_limits_snapshot();
    let mut rate_limits = Vec::new();
    for (user_id, rules) in limits_snapshot.iter() {
        let user_name = user_name_map.get(user_id).cloned().unwrap_or_default();
        for r in rules {
            rate_limits.push(RateLimitToml {
                user_name: user_name.clone(),
                model_pattern: r.model_pattern.clone(),
                rpm: r.rpm,
                rpd: r.rpd,
                total_tokens: r.total_tokens,
            });
        }
    }

    // Quotas
    let quota_map = state.user_quotas_snapshot();
    let quotas: Vec<QuotaToml> = quota_map
        .iter()
        .map(|(user_id, (quota, cost_used))| QuotaToml {
            user_name: user_name_map.get(user_id).cloned().unwrap_or_default(),
            quota: *quota,
            cost_used: *cost_used,
        })
        .collect();

    let toml = GproxyToml {
        global: Some(global),
        providers,
        models,
        model_aliases,
        users,
        permissions,
        file_permissions,
        rate_limits,
        quotas,
    };

    toml::to_string_pretty(&toml).map_err(|e| HttpError::internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// Import: TOML → memory + database
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{UserKeyToml, UserToml};

    #[test]
    fn user_toml_round_trips_argon2_hashes() {
        let hash = crate::login::hash_password("secret-password");
        let user = UserToml {
            name: "alice".to_string(),
            password: hash.clone(),
            enabled: true,
            is_admin: true,
            keys: vec![UserKeyToml {
                api_key: "sk-api01-demo".to_string(),
                label: Some("default".to_string()),
                enabled: false,
            }],
        };

        let toml = toml::to_string(&user).expect("serialize user toml");
        let parsed: UserToml = toml::from_str(&toml).expect("deserialize user toml");

        assert_eq!(
            crate::login::normalize_password_for_storage(&parsed.password),
            hash
        );
        assert_eq!(parsed.keys.len(), 1);
        assert_eq!(parsed.keys[0].api_key, "sk-api01-demo");
        assert_eq!(parsed.keys[0].label.as_deref(), Some("default"));
        assert!(!parsed.keys[0].enabled);
        assert!(parsed.is_admin);
    }
}
