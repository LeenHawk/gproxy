//! Bootstrap logic shared by the startup path and admin reload endpoint.

use std::collections::{HashMap, HashSet};
use std::io::Error as IoError;

use gproxy_sdk::channel::routing::RoutingTableDocument;
use gproxy_sdk::engine::engine::{GproxyEngineBuilder, ProviderConfig};
use gproxy_server::{
    AppState, FilePermissionEntry, GlobalConfig, MemoryClaudeFile, MemoryModel, MemoryUser,
    MemoryUserCredentialFile, MemoryUserKey, PermissionEntry, RateLimitRule,
};
use gproxy_storage::repository::{
    CredentialRepository, ModelRepository, PermissionRepository, ProviderRepository,
    QuotaRepository, SettingsRepository, UserRepository,
};

use crate::admin::config_toml::{GproxyToml, ProviderToml};

/// Counts of items loaded during a reload.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ReloadCounts {
    pub providers: usize,
    pub users: usize,
    pub keys: usize,
    pub models: usize,
    pub user_files: usize,
    pub claude_files: usize,
    pub aliases: usize,
    pub permissions: usize,
    pub file_permissions: usize,
    pub rate_limits: usize,
    pub quotas: usize,
}

#[derive(Debug, Clone)]
pub struct BootstrapAdmin {
    pub username: String,
    pub password: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BootstrapAdminOutcome {
    pub generated_password: Option<String>,
    pub generated_api_key: Option<String>,
}

struct SeedProviderRuntimeState {
    provider_configs: Vec<ProviderConfig>,
    provider_name_to_id: HashMap<String, i64>,
    provider_channel_map: HashMap<String, String>,
    provider_label_map: HashMap<String, Option<String>>,
    provider_credentials: HashMap<String, Vec<i64>>,
    credential_positions: HashMap<i64, (String, usize)>,
}

fn synthetic_provider_id(index: usize) -> i64 {
    index as i64 + 1
}

fn synthetic_credential_id(provider_id: i64, index: usize) -> i64 {
    provider_id * 1000 + index as i64
}

/// Serialize a `ModelPrice` for storage in `models.pricing_json`. Strips
/// `model_id` and `display_name` first because those values live in their own
/// columns and should not be duplicated in the blob — they are restamped on
/// read by `model_rows_to_memory_models`.
pub(crate) fn model_price_to_storage_json(
    price: &gproxy_sdk::channel::billing::ModelPrice,
) -> Result<String, serde_json::Error> {
    let mut storable = price.clone();
    storable.model_id = String::new();
    storable.display_name = None;
    serde_json::to_string(&storable)
}

pub(crate) fn model_rows_to_memory_models(
    models: &[gproxy_storage::ModelQueryRow],
) -> Vec<MemoryModel> {
    models
        .iter()
        .map(|model| {
            let pricing: Option<gproxy_sdk::channel::billing::ModelPrice> = model
                .pricing_json
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .map(|mut parsed: gproxy_sdk::channel::billing::ModelPrice| {
                    parsed.model_id = model.model_id.clone();
                    parsed.display_name = model.display_name.clone();
                    parsed
                });
            MemoryModel {
                id: model.id,
                provider_id: model.provider_id,
                model_id: model.model_id.clone(),
                display_name: model.display_name.clone(),
                enabled: model.enabled,
                pricing,
            }
        })
        .collect()
}

pub(crate) async fn ensure_default_models_in_storage(
    state: &AppState,
    providers: &[(i64, String)],
) -> Result<Vec<gproxy_storage::ModelQueryRow>, Box<dyn std::error::Error + Send + Sync>> {
    let storage = state.storage();
    let existing = storage
        .list_models(&gproxy_storage::ModelQuery::default())
        .await?;
    let mut existing_keys: HashSet<(i64, String)> = existing
        .iter()
        .map(|row| (row.provider_id, row.model_id.clone()))
        .collect();
    let mut next_id = existing.iter().map(|row| row.id).max().unwrap_or(0) + 1;
    let mut inserted = false;

    for (provider_id, channel) in providers {
        let Some(prices) = gproxy_sdk::engine::built_in_model_prices(channel) else {
            continue;
        };
        for price in prices.into_iter().filter(|row| row.model_id != "default") {
            let key = (*provider_id, price.model_id.clone());
            if existing_keys.contains(&key) {
                continue;
            }
            existing_keys.insert(key);

            let pricing_json = Some(model_price_to_storage_json(&price)?);

            storage
                .upsert_model(gproxy_storage::ModelWrite {
                    id: next_id,
                    provider_id: *provider_id,
                    model_id: price.model_id,
                    display_name: price.display_name,
                    enabled: true,
                    pricing_json,
                })
                .await?;
            next_id += 1;
            inserted = true;
        }
    }

    if inserted {
        return Ok(storage
            .list_models(&gproxy_storage::ModelQuery::default())
            .await?);
    }

    Ok(existing)
}

pub fn config_has_enabled_admin_with_key(config: &GproxyToml) -> bool {
    config
        .users
        .iter()
        .any(|user| user.enabled && user.is_admin && user.keys.iter().any(|key| key.enabled))
}

fn next_user_id(state: &AppState) -> i64 {
    state
        .users_snapshot()
        .iter()
        .map(|user| user.id)
        .max()
        .unwrap_or(0)
        + 1
}

fn next_user_key_id(state: &AppState) -> i64 {
    state
        .keys_snapshot()
        .values()
        .map(|key| key.id)
        .max()
        .unwrap_or(0)
        + 1
}

/// Pick the next free user-model-permission id by scanning the in-memory
/// snapshot. Used to allocate an id for the wildcard permission seeded
/// alongside the bootstrap admin so that admin can actually call models
/// out of the box (the policy service has no implicit admin bypass).
fn next_user_permission_id(state: &AppState) -> i64 {
    state
        .user_permissions_snapshot()
        .values()
        .flat_map(|entries| entries.iter().map(|entry| entry.id))
        .max()
        .unwrap_or(0)
        + 1
}

/// Ensure the given user has at least one wildcard model permission
/// (`provider_id = None`, `model_pattern = "*"`). No-op if such an entry
/// already exists. Persisted to both DB and in-memory snapshot so the
/// admin can call models the moment bootstrap finishes.
pub(crate) async fn ensure_user_wildcard_permission(
    state: &AppState,
    user_id: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let already_has_wildcard = state
        .user_permissions_snapshot()
        .get(&user_id)
        .map(|entries| {
            entries
                .iter()
                .any(|entry| entry.provider_id.is_none() && entry.model_pattern == "*")
        })
        .unwrap_or(false);
    if already_has_wildcard {
        return Ok(());
    }

    let permission_id = next_user_permission_id(state);
    state
        .storage()
        .upsert_user_permission(gproxy_storage::UserModelPermissionWrite {
            id: permission_id,
            user_id,
            provider_id: None,
            model_pattern: "*".to_string(),
        })
        .await?;
    state.upsert_permission_in_memory(
        user_id,
        PermissionEntry {
            id: permission_id,
            provider_id: None,
            model_pattern: "*".to_string(),
        },
    );
    Ok(())
}

pub async fn reconcile_bootstrap_admin(
    state: &AppState,
    admin: &BootstrapAdmin,
    generate_missing: bool,
) -> Result<BootstrapAdminOutcome, Box<dyn std::error::Error + Send + Sync>> {
    let existing_user = state
        .users_snapshot()
        .iter()
        .find(|user| user.name == admin.username)
        .cloned();
    let user_id = existing_user
        .as_ref()
        .map(|user| user.id)
        .unwrap_or_else(|| next_user_id(state));

    let mut outcome = BootstrapAdminOutcome::default();
    let password_input = if let Some(password) = admin.password.clone() {
        password
    } else if let Some(user) = &existing_user {
        user.password_hash.clone()
    } else if generate_missing {
        let generated = uuid::Uuid::now_v7().to_string();
        outcome.generated_password = Some(generated.clone());
        generated
    } else {
        return Err(IoError::other(format!(
            "bootstrap admin '{}' requires a password override or an existing stored password",
            admin.username
        ))
        .into());
    };
    let password_hash = crate::login::normalize_password_for_storage(&password_input);

    state
        .storage()
        .upsert_user(gproxy_storage::UserWrite {
            id: user_id,
            name: admin.username.clone(),
            password: password_hash.clone(),
            enabled: true,
            is_admin: true,
        })
        .await?;
    state.upsert_user_in_memory(MemoryUser {
        id: user_id,
        name: admin.username.clone(),
        enabled: true,
        is_admin: true,
        password_hash: password_hash.clone(),
    });

    let existing_user_keys = state.keys_for_user(user_id);
    let (key_id, api_key, label) = if let Some(api_key) = admin.api_key.clone() {
        if let Some(existing_key) = state.authenticate_api_key(&api_key) {
            if existing_key.user_id != user_id {
                return Err(IoError::other(format!(
                    "bootstrap admin API key '{}' is already owned by another user",
                    api_key
                ))
                .into());
            }
            (existing_key.id, api_key, existing_key.label)
        } else {
            (
                next_user_key_id(state),
                api_key,
                Some("bootstrap-admin".to_string()),
            )
        }
    } else if let Some(existing_key) = existing_user_keys
        .iter()
        .find(|key| key.enabled)
        .cloned()
        .or_else(|| existing_user_keys.first().cloned())
    {
        (existing_key.id, existing_key.api_key, existing_key.label)
    } else if generate_missing {
        let generated = crate::admin::users::generate_unique_api_key_for(state);
        outcome.generated_api_key = Some(generated.clone());
        (
            next_user_key_id(state),
            generated,
            Some("bootstrap-admin".to_string()),
        )
    } else {
        return Err(IoError::other(format!(
            "bootstrap admin '{}' requires an API key override or an existing stored API key",
            admin.username
        ))
        .into());
    };

    state
        .storage()
        .upsert_user_key(gproxy_storage::UserKeyWrite {
            id: key_id,
            user_id,
            api_key: api_key.clone(),
            label: label.clone(),
            enabled: true,
        })
        .await?;
    state.upsert_key_in_memory(MemoryUserKey {
        id: key_id,
        user_id,
        api_key,
        label,
        enabled: true,
    });

    // Make sure the bootstrap admin has at least one wildcard model
    // permission so they can actually call providers out of the box.
    // The policy service has no implicit admin bypass — without an
    // explicit `*` row, `check_model_permission` would deny everything.
    ensure_user_wildcard_permission(state, user_id).await?;

    Ok(outcome)
}

fn collect_valid_toml_provider_credentials(
    provider_name: &str,
    channel: &str,
    provider_id: i64,
    credentials: &[serde_json::Value],
) -> Vec<(i64, serde_json::Value)> {
    credentials
        .iter()
        .enumerate()
        .filter_map(|(credential_index, credential)| {
            let credential_id = synthetic_credential_id(provider_id, credential_index);
            match gproxy_sdk::engine::engine::validate_credential_json(channel, credential) {
                Ok(()) => Some((credential_id, credential.clone())),
                Err(err) => {
                    tracing::warn!(
                        provider = provider_name,
                        credential_id,
                        error = %err,
                        "skipping invalid provider credential during seed"
                    );
                    None
                }
            }
        })
        .collect()
}

pub(crate) fn collect_valid_db_provider_credentials(
    provider_name: &str,
    channel: &str,
    credentials: &[gproxy_storage::CredentialQueryRow],
) -> Vec<(i64, serde_json::Value)> {
    credentials
        .iter()
        .filter_map(|credential| {
            match gproxy_sdk::engine::engine::validate_credential_json(
                channel,
                &credential.secret_json,
            ) {
                Ok(()) => Some((credential.id, credential.secret_json.clone())),
                Err(err) => {
                    tracing::warn!(
                        provider = provider_name,
                        credential_id = credential.id,
                        error = %err,
                        "skipping invalid provider credential during runtime load"
                    );
                    None
                }
            }
        })
        .collect()
}

fn build_seed_provider_runtime_state(providers: &[ProviderToml]) -> SeedProviderRuntimeState {
    let mut provider_configs = Vec::new();
    let mut provider_name_to_id = HashMap::new();
    let mut provider_channel_map = HashMap::new();
    let mut provider_label_map = HashMap::new();
    let mut provider_credentials = HashMap::new();
    let mut credential_positions = HashMap::new();

    for (provider_index, provider) in providers.iter().enumerate() {
        let provider_id = synthetic_provider_id(provider_index);
        provider_name_to_id.insert(provider.name.clone(), provider_id);
        let valid_credentials = collect_valid_toml_provider_credentials(
            &provider.name,
            &provider.channel,
            provider_id,
            &provider.credentials,
        );
        provider_configs.push(ProviderConfig {
            name: provider.name.clone(),
            channel: provider.channel.clone(),
            settings_json: provider.settings.clone(),
            credentials: valid_credentials
                .iter()
                .map(|(_, credential)| credential.clone())
                .collect(),
            routing: None,
        });
        provider_channel_map.insert(provider.name.clone(), provider.channel.clone());
        provider_label_map.insert(provider.name.clone(), provider.label.clone());

        let credential_ids: Vec<i64> = valid_credentials
            .iter()
            .map(|(credential_id, _)| *credential_id)
            .collect();
        for (credential_index, credential_id) in credential_ids.iter().copied().enumerate() {
            credential_positions.insert(credential_id, (provider.name.clone(), credential_index));
        }
        provider_credentials.insert(provider.name.clone(), credential_ids);
    }

    SeedProviderRuntimeState {
        provider_configs,
        provider_name_to_id,
        provider_channel_map,
        provider_label_map,
        provider_credentials,
        credential_positions,
    }
}

pub(crate) async fn apply_persisted_credential_statuses(
    state: &AppState,
    credential_positions: &HashMap<i64, (String, usize)>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if credential_positions.is_empty() {
        return Ok(());
    }

    let statuses = state
        .storage()
        .list_credential_statuses(&gproxy_storage::CredentialStatusQuery::default())
        .await?;
    let store = state.engine().store().clone();

    for status in statuses {
        let Some((provider_name, index)) = credential_positions.get(&status.credential_id) else {
            continue;
        };
        match status.health_kind.as_str() {
            "dead" => {
                store.mark_credential_dead(provider_name, *index);
            }
            "healthy" => {
                store.mark_credential_healthy(provider_name, *index);
            }
            _ => {}
        }
    }

    Ok(())
}

/// Reload all in-memory state from the database.
///
/// Used by both the initial bootstrap and the `POST /admin/reload` endpoint.
pub async fn reload_from_db(
    state: &AppState,
) -> Result<ReloadCounts, Box<dyn std::error::Error + Send + Sync>> {
    let storage = state.storage();

    // Phase 1: read and build everything from the DB without mutating memory.
    let replacement_config = storage
        .get_global_settings()
        .await?
        .map(|settings| GlobalConfig {
            host: settings.host,
            port: settings.port as u16,
            proxy: settings.proxy,
            spoof_emulation: settings.spoof_emulation.unwrap_or_default(),
            enable_usage: settings.enable_usage,
            enable_upstream_log: settings.enable_upstream_log,
            enable_upstream_log_body: settings.enable_upstream_log_body,
            enable_downstream_log: settings.enable_downstream_log,
            enable_downstream_log_body: settings.enable_downstream_log_body,
            dsn: settings.dsn,
            data_dir: settings.data_dir,
        });
    let config = replacement_config
        .clone()
        .unwrap_or_else(|| (*state.config()).clone());

    let providers = storage
        .list_providers(&gproxy_storage::ProviderQuery::default())
        .await?;
    let all_credentials = storage
        .list_credentials(&gproxy_storage::CredentialQuery::default())
        .await?;
    let users = storage
        .list_users(&gproxy_storage::UserQuery::default())
        .await?;
    let keys = storage.list_user_keys_for_memory().await?;
    let models = storage
        .list_models(&gproxy_storage::ModelQuery::default())
        .await?;
    let perms = storage
        .list_user_model_permissions(&gproxy_storage::UserModelPermissionQuery::default())
        .await?;
    let file_permissions = storage
        .list_user_file_permissions(&gproxy_storage::UserFilePermissionQuery::default())
        .await?;
    let limits = storage
        .list_user_rate_limits(&gproxy_storage::UserRateLimitQuery::default())
        .await?;
    let quotas = storage.list_user_quotas().await?;
    let user_files = storage
        .list_user_credential_files(&gproxy_storage::UserCredentialFileQuery::default())
        .await?;
    let claude_files = storage
        .list_claude_files(&gproxy_storage::ClaudeFileQuery::default())
        .await?;

    let mut builder = GproxyEngineBuilder::new().configure_clients(
        config.proxy.as_deref(),
        Some(config.spoof_emulation.as_str()),
    );
    let valid_credentials_by_provider: HashMap<i64, Vec<(i64, serde_json::Value)>> = providers
        .iter()
        .map(|provider| {
            let credentials: Vec<_> = all_credentials
                .iter()
                .filter(|credential| credential.provider_id == provider.id && credential.enabled)
                .cloned()
                .collect();
            (
                provider.id,
                collect_valid_db_provider_credentials(
                    &provider.name,
                    &provider.channel,
                    &credentials,
                ),
            )
        })
        .collect();
    let mut provider_count = 0;
    for provider in &providers {
        let creds: Vec<serde_json::Value> = valid_credentials_by_provider
            .get(&provider.id)
            .into_iter()
            .flatten()
            .map(|(_, credential)| credential.clone())
            .collect();
        builder = builder.add_provider_json(ProviderConfig {
            name: provider.name.clone(),
            channel: provider.channel.clone(),
            settings_json: provider.settings_json.clone(),
            credentials: creds,
            routing: RoutingTableDocument::from_json_value(provider.routing_json.clone())?,
        })?;
        provider_count += 1;
    }
    let engine = builder.build();

    let credential_positions: HashMap<i64, (String, usize)> = providers
        .iter()
        .flat_map(|provider| {
            valid_credentials_by_provider
                .get(&provider.id)
                .into_iter()
                .flat_map(move |credentials| {
                    credentials
                        .iter()
                        .enumerate()
                        .map(move |(index, (credential_id, _))| {
                            (*credential_id, (provider.name.clone(), index))
                        })
                })
        })
        .collect();
    let credential_statuses = if credential_positions.is_empty() {
        Vec::new()
    } else {
        storage
            .list_credential_statuses(&gproxy_storage::CredentialStatusQuery::default())
            .await?
    };
    let engine_store = engine.store().clone();
    for status in credential_statuses {
        let Some((provider_name, index)) = credential_positions.get(&status.credential_id) else {
            continue;
        };
        match status.health_kind.as_str() {
            "dead" => {
                engine_store.mark_credential_dead(provider_name, *index);
            }
            "healthy" => {
                engine_store.mark_credential_healthy(provider_name, *index);
            }
            _ => {}
        }
    }
    let provider_credentials: HashMap<String, Vec<i64>> = providers
        .iter()
        .map(|provider| {
            let ids = valid_credentials_by_provider
                .get(&provider.id)
                .into_iter()
                .flatten()
                .map(|(credential_id, _)| *credential_id)
                .collect();
            (provider.name.clone(), ids)
        })
        .collect();
    let provider_name_map: HashMap<String, i64> = providers
        .iter()
        .map(|provider| (provider.name.clone(), provider.id))
        .collect();
    let provider_channel_map: HashMap<String, String> = providers
        .iter()
        .map(|provider| (provider.name.clone(), provider.channel.clone()))
        .collect();
    let provider_label_map: HashMap<String, Option<String>> = providers
        .iter()
        .map(|provider| (provider.name.clone(), provider.label.clone()))
        .collect();

    let user_count = users.len();
    let memory_users: Vec<MemoryUser> = users
        .iter()
        .map(|user| MemoryUser {
            id: user.id,
            name: user.name.clone(),
            enabled: user.enabled,
            is_admin: user.is_admin,
            password_hash: user.password.clone(),
        })
        .collect();

    let key_count = keys.len();
    let memory_keys: Vec<MemoryUserKey> = keys
        .iter()
        .map(|key| MemoryUserKey {
            id: key.id,
            user_id: key.user_id,
            api_key: key.api_key.clone(),
            label: key.label.clone(),
            enabled: key.enabled,
        })
        .collect();

    let model_count = models.len();
    let memory_models: Vec<MemoryModel> = model_rows_to_memory_models(&models);

    let perm_count = perms.len();
    let mut perm_map: HashMap<i64, Vec<PermissionEntry>> = HashMap::new();
    for permission in perms {
        perm_map
            .entry(permission.user_id)
            .or_default()
            .push(PermissionEntry {
                id: permission.id,
                provider_id: permission.provider_id,
                model_pattern: permission.model_pattern,
            });
    }

    let file_permission_count = file_permissions.len();
    let mut file_permission_map: HashMap<i64, Vec<FilePermissionEntry>> = HashMap::new();
    for permission in file_permissions {
        file_permission_map
            .entry(permission.user_id)
            .or_default()
            .push(FilePermissionEntry {
                id: permission.id,
                provider_id: permission.provider_id,
            });
    }

    let limit_count = limits.len();
    let mut limit_map: HashMap<i64, Vec<RateLimitRule>> = HashMap::new();
    for limit in limits {
        limit_map
            .entry(limit.user_id)
            .or_default()
            .push(RateLimitRule {
                id: limit.id,
                model_pattern: limit.model_pattern,
                rpm: limit.rpm,
                rpd: limit.rpd,
                total_tokens: limit.total_tokens,
            });
    }

    let quota_count = quotas.len();
    let quota_map: HashMap<i64, (f64, f64)> = quotas
        .into_iter()
        .map(|quota| (quota.user_id, (quota.quota, quota.cost_used)))
        .collect();

    let user_file_count = user_files.len();
    let memory_user_files: Vec<MemoryUserCredentialFile> = user_files
        .into_iter()
        .map(|file| MemoryUserCredentialFile {
            user_id: file.user_id,
            user_key_id: file.user_key_id,
            provider_id: file.provider_id,
            credential_id: file.credential_id,
            file_id: file.file_id,
            active: file.active,
            created_at_unix_ms: file.created_at.unix_timestamp_nanos() as i64 / 1_000_000,
        })
        .collect();

    let claude_file_count = claude_files.len();
    let claude_file_map: HashMap<(i64, String), MemoryClaudeFile> = claude_files
        .into_iter()
        .filter_map(|file| {
            let metadata = serde_json::from_value::<
                gproxy_sdk::protocol::claude::types::FileMetadata,
            >(file.raw_json)
            .ok()?;
            let file_created_at_unix_ms = time::OffsetDateTime::parse(
                &file.file_created_at,
                &time::format_description::well_known::Rfc3339,
            )
            .map(|dt| dt.unix_timestamp_nanos() as i64 / 1_000_000)
            .unwrap_or_default();
            Some((
                (file.provider_id, file.file_id.clone()),
                MemoryClaudeFile {
                    provider_id: file.provider_id,
                    file_id: file.file_id,
                    file_created_at_unix_ms,
                    metadata,
                },
            ))
        })
        .collect();

    // Phase 2: commit the fully prepared replacement state to memory.
    if let Some(config) = replacement_config {
        state.replace_config(config);
    }
    state.replace_engine(engine);
    state.replace_provider_credentials(provider_credentials);
    state.replace_provider_names(provider_name_map);
    state.replace_provider_channels(provider_channel_map);
    state.replace_provider_labels(provider_label_map);
    state.replace_users(memory_users);
    state.replace_keys(memory_keys);
    state.replace_models(memory_models);
    // Push pricing into the engine so admin-edited prices take effect at billing time.
    for provider in &providers {
        state.push_pricing_to_engine(&provider.name);
    }
    state.replace_user_permissions(perm_map);
    state.replace_user_file_permissions(file_permission_map);
    state.replace_user_rate_limits(limit_map);
    state.replace_user_quotas(quota_map);
    state.replace_user_files(memory_user_files);
    state.replace_claude_files(claude_file_map);

    Ok(ReloadCounts {
        providers: provider_count,
        users: user_count,
        keys: key_count,
        models: model_count,
        user_files: user_file_count,
        claude_files: claude_file_count,
        aliases: 0,
        permissions: perm_count,
        file_permissions: file_permission_count,
        rate_limits: limit_count,
        quotas: quota_count,
    })
}

/// Seed startup state from a TOML config string and persist it to the database.
pub async fn seed_from_toml(
    state: &AppState,
    toml_str: &str,
) -> Result<BootstrapAdminOutcome, Box<dyn std::error::Error + Send + Sync>> {
    seed_from_toml_with_bootstrap(state, toml_str, None).await
}

pub async fn seed_from_toml_with_bootstrap(
    state: &AppState,
    toml_str: &str,
    bootstrap_admin: Option<&BootstrapAdmin>,
) -> Result<BootstrapAdminOutcome, Box<dyn std::error::Error + Send + Sync>> {
    let config: GproxyToml = toml::from_str(toml_str)?;

    // 1. Global settings → memory + DB
    if let Some(gs) = &config.global {
        let gc = GlobalConfig {
            host: gs.host.clone(),
            port: gs.port,
            proxy: gs.proxy.clone(),
            spoof_emulation: gs.spoof_emulation.clone(),
            enable_usage: gs.enable_usage,
            enable_upstream_log: gs.enable_upstream_log,
            enable_upstream_log_body: gs.enable_upstream_log_body,
            enable_downstream_log: gs.enable_downstream_log,
            enable_downstream_log_body: gs.enable_downstream_log_body,
            dsn: gs.dsn.clone(),
            data_dir: gs.data_dir.clone(),
        };
        state
            .storage()
            .upsert_global_settings(gproxy_storage::GlobalSettingsWrite {
                host: gc.host.clone(),
                port: gc.port,
                proxy: gc.proxy.clone(),
                spoof_emulation: gc.spoof_emulation.clone(),
                enable_usage: gc.enable_usage,
                enable_upstream_log: gc.enable_upstream_log,
                enable_upstream_log_body: gc.enable_upstream_log_body,
                enable_downstream_log: gc.enable_downstream_log,
                enable_downstream_log_body: gc.enable_downstream_log_body,
                dsn: gc.dsn.clone(),
                data_dir: gc.data_dir.clone(),
            })
            .await?;
        state.replace_config(gc);
    } else {
        let cfg = state.config().clone();
        state
            .storage()
            .upsert_global_settings(gproxy_storage::GlobalSettingsWrite {
                host: cfg.host.clone(),
                port: cfg.port,
                proxy: cfg.proxy.clone(),
                spoof_emulation: cfg.spoof_emulation.clone(),
                enable_usage: cfg.enable_usage,
                enable_upstream_log: cfg.enable_upstream_log,
                enable_upstream_log_body: cfg.enable_upstream_log_body,
                enable_downstream_log: cfg.enable_downstream_log,
                enable_downstream_log_body: cfg.enable_downstream_log_body,
                dsn: cfg.dsn.clone(),
                data_dir: cfg.data_dir.clone(),
            })
            .await?;
    }

    // 2. Providers → engine + DB
    let proxy = config.global.as_ref().and_then(|g| g.proxy.as_deref());
    let spoof = config.global.as_ref().map(|g| g.spoof_emulation.as_str());
    let provider_runtime = build_seed_provider_runtime_state(&config.providers);
    let mut builder = GproxyEngineBuilder::new().configure_clients(proxy, spoof);
    for provider_config in provider_runtime.provider_configs {
        builder = builder.add_provider_json(provider_config)?;
    }
    for (i, p) in config.providers.iter().enumerate() {
        let provider_id = synthetic_provider_id(i);
        // Persist provider
        state
            .storage()
            .upsert_provider(gproxy_storage::ProviderWrite {
                id: provider_id,
                name: p.name.clone(),
                channel: p.channel.clone(),
                label: p.label.clone(),
                settings_json: serde_json::to_string(&p.settings).unwrap_or_default(),
                routing_json: String::new(),
            })
            .await?;
        // Persist credentials
        for (credential_id, credential) in collect_valid_toml_provider_credentials(
            &p.name,
            &p.channel,
            provider_id,
            &p.credentials,
        ) {
            state
                .storage()
                .upsert_credential(gproxy_storage::CredentialWrite {
                    id: credential_id,
                    provider_id,
                    name: None,
                    kind: p.channel.clone(),
                    enabled: true,
                    secret_json: serde_json::to_string(&credential).unwrap_or_default(),
                })
                .await?;
        }
    }
    let persisted_provider_rows = state
        .storage()
        .list_providers(&gproxy_storage::ProviderQuery::default())
        .await?;
    let persisted_models = ensure_default_models_in_storage(
        state,
        &persisted_provider_rows
            .iter()
            .map(|provider| (provider.id, provider.channel.clone()))
            .collect::<Vec<_>>(),
    )
    .await?;
    state.replace_engine(builder.build());
    apply_persisted_credential_statuses(state, &provider_runtime.credential_positions).await?;

    state.replace_provider_names(provider_runtime.provider_name_to_id.clone());
    state.replace_provider_channels(provider_runtime.provider_channel_map);
    state.replace_provider_labels(provider_runtime.provider_label_map);
    state.replace_provider_credentials(provider_runtime.provider_credentials);

    // 3. Users → memory + DB
    for (i, u) in config.users.iter().enumerate() {
        let user_id = i as i64 + 1;
        let hashed_password = crate::login::normalize_password_for_storage(&u.password);
        state
            .storage()
            .upsert_user(gproxy_storage::UserWrite {
                id: user_id,
                name: u.name.clone(),
                password: hashed_password.clone(),
                enabled: u.enabled,
                is_admin: u.is_admin,
            })
            .await?;
        state.upsert_user_in_memory(MemoryUser {
            id: user_id,
            name: u.name.clone(),
            enabled: u.enabled,
            is_admin: u.is_admin,
            password_hash: hashed_password.clone(),
        });
        for (j, key) in u.keys.iter().enumerate() {
            let key_id = user_id * 1000 + j as i64;
            // Check for duplicate keys across users
            if state.authenticate_api_key(&key.api_key).is_some() {
                tracing::warn!(
                    user = %u.name,
                    "TOML key already exists — skipping duplicate"
                );
                continue;
            }
            state
                .storage()
                .upsert_user_key(gproxy_storage::UserKeyWrite {
                    id: key_id,
                    user_id,
                    api_key: key.api_key.clone(),
                    label: key.label.clone(),
                    enabled: key.enabled,
                })
                .await?;
            state.upsert_key_in_memory(MemoryUserKey {
                id: key_id,
                user_id,
                api_key: key.api_key.clone(),
                label: key.label.clone(),
                enabled: key.enabled,
            });
        }
    }

    let outcome = if let Some(bootstrap_admin) = bootstrap_admin {
        reconcile_bootstrap_admin(state, bootstrap_admin, true).await?
    } else {
        BootstrapAdminOutcome::default()
    };

    // 4. Models → memory + DB
    let explicit_models: Vec<MemoryModel> = config
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let provider_id = provider_runtime
                .provider_name_to_id
                .get(&m.provider_name)
                .copied()
                .unwrap_or(0);
            let has_any_pricing = m.price_each_call.is_some()
                || !m.price_tiers.is_empty()
                || m.flex_price_each_call.is_some()
                || !m.flex_price_tiers.is_empty()
                || m.scale_price_each_call.is_some()
                || !m.scale_price_tiers.is_empty()
                || m.priority_price_each_call.is_some()
                || !m.priority_price_tiers.is_empty();
            let pricing = if has_any_pricing {
                Some(gproxy_sdk::channel::billing::ModelPrice {
                    model_id: m.model_id.clone(),
                    display_name: m.display_name.clone(),
                    price_each_call: m.price_each_call,
                    price_tiers: m.price_tiers.clone(),
                    flex_price_each_call: m.flex_price_each_call,
                    flex_price_tiers: m.flex_price_tiers.clone(),
                    scale_price_each_call: m.scale_price_each_call,
                    scale_price_tiers: m.scale_price_tiers.clone(),
                    priority_price_each_call: m.priority_price_each_call,
                    priority_price_tiers: m.priority_price_tiers.clone(),
                })
            } else {
                None
            };
            MemoryModel {
                id: persisted_models.iter().map(|row| row.id).max().unwrap_or(0) + i as i64 + 1,
                provider_id,
                model_id: m.model_id.clone(),
                display_name: m.display_name.clone(),
                enabled: m.enabled,
                pricing,
            }
        })
        .collect();
    for m in &explicit_models {
        let pricing_json = m
            .pricing
            .as_ref()
            .map(model_price_to_storage_json)
            .transpose()?;
        state
            .storage()
            .upsert_model(gproxy_storage::ModelWrite {
                id: m.id,
                provider_id: m.provider_id,
                model_id: m.model_id.clone(),
                display_name: m.display_name.clone(),
                enabled: m.enabled,
                pricing_json,
            })
            .await?;
    }
    let all_models = state
        .storage()
        .list_models(&gproxy_storage::ModelQuery::default())
        .await?;
    state.replace_models(model_rows_to_memory_models(&all_models));
    // Push pricing into the engine so admin-edited prices take effect at billing time.
    for provider_name in provider_runtime.provider_name_to_id.keys() {
        state.push_pricing_to_engine(provider_name);
    }

    // 5. Permissions, file permissions, rate limits, quotas → memory + DB
    let users_snapshot = state.users_snapshot();
    let user_id_map: HashMap<String, i64> = users_snapshot
        .iter()
        .map(|u| (u.name.clone(), u.id))
        .collect();

    let mut perm_writes: HashMap<(i64, Option<i64>, String), PermissionEntry> = HashMap::new();
    for (i, p) in config.permissions.iter().enumerate() {
        if let Some(&user_id) = user_id_map.get(&p.user_name) {
            let provider_id = p
                .provider_name
                .as_ref()
                .and_then(|name| provider_runtime.provider_name_to_id.get(name).copied());
            perm_writes.insert(
                (user_id, provider_id, p.model_pattern.clone()),
                PermissionEntry {
                    id: i as i64 + 1,
                    provider_id,
                    model_pattern: p.model_pattern.clone(),
                },
            );
        }
    }
    let mut perm_map: HashMap<i64, Vec<PermissionEntry>> = HashMap::new();
    for ((user_id, provider_id, model_pattern), entry) in perm_writes {
        state
            .storage()
            .upsert_user_permission(gproxy_storage::UserModelPermissionWrite {
                id: entry.id,
                user_id,
                provider_id,
                model_pattern,
            })
            .await?;
        perm_map.entry(user_id).or_default().push(entry);
    }
    state.replace_user_permissions(perm_map);

    // The TOML permission replace above wipes the in-memory snapshot,
    // including the wildcard entry seeded by `reconcile_bootstrap_admin`.
    // Re-seed it so a TOML bootstrap that doesn't list any permissions
    // still leaves the admin able to call models.
    if let Some(bootstrap_admin) = bootstrap_admin
        && let Some(admin_user) = state
            .users_snapshot()
            .iter()
            .find(|user| user.name == bootstrap_admin.username)
            .cloned()
    {
        ensure_user_wildcard_permission(state, admin_user.id).await?;
    }

    let mut file_permission_writes: HashMap<(i64, i64), FilePermissionEntry> = HashMap::new();
    for (i, permission) in config.file_permissions.iter().enumerate() {
        let Some(&user_id) = user_id_map.get(&permission.user_name) else {
            continue;
        };
        let Some(&provider_id) = provider_runtime
            .provider_name_to_id
            .get(&permission.provider_name)
        else {
            continue;
        };
        file_permission_writes.insert(
            (user_id, provider_id),
            FilePermissionEntry {
                id: i as i64 + 1,
                provider_id,
            },
        );
    }
    let mut file_permission_map: HashMap<i64, Vec<FilePermissionEntry>> = HashMap::new();
    for ((user_id, provider_id), entry) in file_permission_writes {
        state
            .storage()
            .upsert_user_file_permission(gproxy_storage::UserFilePermissionWrite {
                id: entry.id,
                user_id,
                provider_id,
            })
            .await?;
        file_permission_map.entry(user_id).or_default().push(entry);
    }
    state.replace_user_file_permissions(file_permission_map);

    let mut limit_map: HashMap<i64, Vec<RateLimitRule>> = HashMap::new();
    for (i, r) in config.rate_limits.iter().enumerate() {
        if let Some(&user_id) = user_id_map.get(&r.user_name) {
            limit_map.entry(user_id).or_default().push(RateLimitRule {
                id: (i + 1) as i64,
                model_pattern: r.model_pattern.clone(),
                rpm: r.rpm,
                rpd: r.rpd,
                total_tokens: r.total_tokens,
            });
            state
                .storage()
                .upsert_user_rate_limit(gproxy_storage::UserRateLimitWrite {
                    id: i as i64 + 1,
                    user_id,
                    model_pattern: r.model_pattern.clone(),
                    rpm: r.rpm,
                    rpd: r.rpd,
                    total_tokens: r.total_tokens,
                })
                .await?;
        }
    }
    state.replace_user_rate_limits(limit_map);

    let mut quota_map: HashMap<i64, (f64, f64)> = HashMap::new();
    for q in &config.quotas {
        if let Some(&user_id) = user_id_map.get(&q.user_name) {
            state
                .storage()
                .upsert_user_quota(gproxy_storage::UserQuotaWrite {
                    user_id,
                    quota: q.quota,
                    cost_used: q.cost_used,
                })
                .await?;
            quota_map.insert(user_id, (q.quota, q.cost_used));
        }
    }
    state.replace_user_quotas(quota_map);

    Ok(outcome)
}

/// Seed the database with minimal defaults (global_settings only).
pub async fn seed_defaults(
    state: &AppState,
    bootstrap_admin: &BootstrapAdmin,
) -> Result<BootstrapAdminOutcome, Box<dyn std::error::Error + Send + Sync>> {
    let cfg = state.config().clone();
    state
        .storage()
        .upsert_global_settings(gproxy_storage::GlobalSettingsWrite {
            host: cfg.host.clone(),
            port: cfg.port,
            proxy: cfg.proxy.clone(),
            spoof_emulation: cfg.spoof_emulation.clone(),
            enable_usage: cfg.enable_usage,
            enable_upstream_log: cfg.enable_upstream_log,
            enable_upstream_log_body: cfg.enable_upstream_log_body,
            enable_downstream_log: cfg.enable_downstream_log,
            enable_downstream_log_body: cfg.enable_downstream_log_body,
            dsn: cfg.dsn.clone(),
            data_dir: cfg.data_dir.clone(),
        })
        .await?;

    reconcile_bootstrap_admin(state, bootstrap_admin, true).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sea_orm::ConnectionTrait;
    use serde_json::json;

    use super::{
        BootstrapAdmin, build_seed_provider_runtime_state, config_has_enabled_admin_with_key,
        reconcile_bootstrap_admin, reload_from_db, seed_from_toml_with_bootstrap,
    };
    use crate::admin::config_toml::{GproxyToml, ProviderToml, UserKeyToml, UserToml};
    use gproxy_server::{AppStateBuilder, GlobalConfig, MemoryUser, MemoryUserKey};
    use gproxy_storage::{
        GlobalSettingsWrite, Scope, SeaOrmStorage, SettingsRepository, UserKeyQuery, UserRepository,
    };

    #[test]
    fn seed_provider_runtime_matches_reload_shape() {
        let state = build_seed_provider_runtime_state(&[
            ProviderToml {
                name: "first".to_string(),
                channel: "anthropic".to_string(),
                label: None,
                settings: json!({"region": "us"}),
                credentials: vec![json!({"api_key": "key-1"})],
            },
            ProviderToml {
                name: "second".to_string(),
                channel: "claudecode".to_string(),
                label: Some("Second (EU)".to_string()),
                settings: json!({"region": "eu"}),
                credentials: vec![
                    json!({"access_token": "key-2"}),
                    json!({"access_token": "key-3"}),
                ],
            },
        ]);

        assert_eq!(state.provider_configs.len(), 2);
        assert_eq!(state.provider_configs[0].name, "first");
        assert_eq!(state.provider_configs[1].name, "second");

        assert_eq!(state.provider_name_to_id.get("first"), Some(&1));
        assert_eq!(state.provider_name_to_id.get("second"), Some(&2));

        assert_eq!(
            state.provider_channel_map.get("first").map(String::as_str),
            Some("anthropic")
        );
        assert_eq!(
            state.provider_channel_map.get("second").map(String::as_str),
            Some("claudecode")
        );

        assert_eq!(state.provider_label_map.get("first"), Some(&None));
        assert_eq!(
            state.provider_label_map.get("second"),
            Some(&Some("Second (EU)".to_string()))
        );

        assert_eq!(state.provider_credentials.get("first"), Some(&vec![1000]));
        assert_eq!(
            state.provider_credentials.get("second"),
            Some(&vec![2000, 2001])
        );
        assert_eq!(
            state.credential_positions.get(&1000),
            Some(&("first".to_string(), 0))
        );
        assert_eq!(
            state.credential_positions.get(&2000),
            Some(&("second".to_string(), 0))
        );
        assert_eq!(
            state.credential_positions.get(&2001),
            Some(&("second".to_string(), 1))
        );
    }

    #[test]
    fn seed_provider_runtime_skips_invalid_credentials_in_mapping() {
        let state = build_seed_provider_runtime_state(&[ProviderToml {
            name: "openai".to_string(),
            channel: "openai".to_string(),
            label: None,
            settings: json!({}),
            credentials: vec![json!({"api_key": "sk-good"}), json!({"token": "bad"})],
        }]);

        assert_eq!(state.provider_configs.len(), 1);
        assert_eq!(state.provider_configs[0].credentials.len(), 1);
        assert_eq!(state.provider_credentials.get("openai"), Some(&vec![1000]));
        assert_eq!(
            state.credential_positions.get(&1000),
            Some(&("openai".to_string(), 0))
        );
        assert!(!state.credential_positions.contains_key(&1001));
    }

    #[tokio::test]
    async fn reload_from_db_keeps_memory_unchanged_when_a_late_db_read_fails() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");

        let state = AppStateBuilder::new()
            .engine(gproxy_sdk::engine::engine::GproxyEngine::builder().build())
            .storage(storage.clone())
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .users(vec![MemoryUser {
                id: 9,
                name: "memory-user".to_string(),
                enabled: true,
                is_admin: true,
                password_hash: "memory-hash".to_string(),
            }])
            .keys(vec![MemoryUserKey {
                id: 99,
                user_id: 9,
                api_key: "memory-key".to_string(),
                label: Some("memory-label".to_string()),
                enabled: true,
            }])
            .build();

        storage
            .upsert_global_settings(GlobalSettingsWrite {
                host: "127.0.0.1".to_string(),
                port: 8787,
                proxy: Some("http://db-proxy".to_string()),
                spoof_emulation: "chrome_136".to_string(),
                enable_usage: false,
                enable_upstream_log: true,
                enable_upstream_log_body: true,
                enable_downstream_log: true,
                enable_downstream_log_body: true,
                dsn: "sqlite::memory:".to_string(),
                data_dir: "/tmp/db-data".to_string(),
            })
            .await
            .expect("seed global settings");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "db-user".to_string(),
                password: "db-password".to_string(),
                enabled: false,
                is_admin: false,
            })
            .await
            .expect("seed db user");

        storage
            .connection()
            .execute_unprepared("DROP TABLE claude_files")
            .await
            .expect("drop late-read table");

        reload_from_db(&state)
            .await
            .expect_err("reload should fail after the late table drop");

        assert_eq!(state.config().dsn, "sqlite::memory:");
        assert_eq!(state.config().proxy, None);

        let users = state.users_snapshot();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, 9);
        assert_eq!(users[0].name, "memory-user");
        assert!(users[0].is_admin);

        let keys = state.keys_for_user(9);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, 99);
        assert_eq!(keys[0].api_key, "memory-key");
    }

    #[tokio::test]
    async fn seed_defaults_creates_real_admin_user_and_key() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");

        let state = AppStateBuilder::new()
            .engine(gproxy_sdk::engine::engine::GproxyEngine::builder().build())
            .storage(storage.clone())
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .build();

        super::seed_defaults(
            &state,
            &BootstrapAdmin {
                username: "admin".to_string(),
                password: Some("secret-password".to_string()),
                api_key: Some("sk-admin".to_string()),
            },
        )
        .await
        .expect("seed defaults");

        let users = storage
            .list_users(&gproxy_storage::UserQuery::default())
            .await
            .expect("query users");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "admin");
        assert!(users[0].enabled);
        assert!(users[0].is_admin);

        let keys = storage
            .list_user_keys(&UserKeyQuery {
                user_id: Scope::Eq(users[0].id),
                ..Default::default()
            })
            .await
            .expect("query user keys");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].api_key, "sk-admin");
        assert!(keys[0].enabled);
    }

    #[tokio::test]
    async fn seed_from_toml_bootstraps_admin_when_toml_has_no_admin_user() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");

        let state = AppStateBuilder::new()
            .engine(gproxy_sdk::engine::engine::GproxyEngine::builder().build())
            .storage(storage.clone())
            .config(GlobalConfig {
                dsn: "sqlite://data/gproxy.db?mode=rwc".to_string(),
                data_dir: "./data".to_string(),
                ..GlobalConfig::default()
            })
            .build();

        let toml_str = r#"
[global]
host = "127.0.0.1"
port = 8787
dsn = "sqlite://data/gproxy.db?mode=rwc"
data_dir = "./data"

[[users]]
name = "alice"
password = "alice-password"
enabled = true
"#;

        seed_from_toml_with_bootstrap(
            &state,
            toml_str,
            Some(&BootstrapAdmin {
                username: "admin".to_string(),
                password: Some("bootstrap-password".to_string()),
                api_key: Some("sk-bootstrap-admin".to_string()),
            }),
        )
        .await
        .expect("seed from toml");

        let users = storage
            .list_users(&gproxy_storage::UserQuery::default())
            .await
            .expect("query users");
        assert_eq!(users.len(), 2);
        assert!(
            users
                .iter()
                .any(|user| user.name == "admin" && user.is_admin)
        );

        let admin = users
            .iter()
            .find(|user| user.name == "admin")
            .expect("admin user");
        let keys = storage
            .list_user_keys(&UserKeyQuery {
                user_id: Scope::Eq(admin.id),
                ..Default::default()
            })
            .await
            .expect("query user keys");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].api_key, "sk-bootstrap-admin");

        // The bootstrap admin must come with a wildcard model permission
        // — without it `check_model_permission` would reject every call
        // until the operator manually adds a row.
        let admin_perms = state
            .user_permissions_snapshot()
            .get(&admin.id)
            .cloned()
            .expect("bootstrap admin should have permissions seeded");
        assert!(
            admin_perms
                .iter()
                .any(|entry| entry.provider_id.is_none() && entry.model_pattern == "*"),
            "bootstrap admin missing wildcard permission: {:?}",
            admin_perms
        );
    }

    #[test]
    fn config_has_enabled_admin_with_key_matches_expected_shape() {
        let config = GproxyToml {
            global: None,
            providers: Vec::new(),
            models: Vec::new(),
            users: vec![
                UserToml {
                    name: "alice".to_string(),
                    password: "pw".to_string(),
                    enabled: true,
                    is_admin: false,
                    keys: vec![],
                },
                UserToml {
                    name: "admin".to_string(),
                    password: "pw".to_string(),
                    enabled: true,
                    is_admin: true,
                    keys: vec![UserKeyToml {
                        api_key: "sk-admin".to_string(),
                        label: None,
                        enabled: true,
                    }],
                },
            ],
            permissions: Vec::new(),
            file_permissions: Vec::new(),
            rate_limits: Vec::new(),
            quotas: Vec::new(),
        };

        assert!(config_has_enabled_admin_with_key(&config));
    }

    #[tokio::test]
    async fn reconcile_bootstrap_admin_overwrites_named_user_and_preserves_other_admins() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");

        let state = Arc::new(
            AppStateBuilder::new()
                .engine(gproxy_sdk::engine::engine::GproxyEngine::builder().build())
                .storage(storage.clone())
                .config(GlobalConfig {
                    dsn: "sqlite::memory:".to_string(),
                    ..GlobalConfig::default()
                })
                .users(vec![
                    MemoryUser {
                        id: 1,
                        name: "admin".to_string(),
                        enabled: false,
                        is_admin: false,
                        password_hash: crate::login::hash_password("old-password"),
                    },
                    MemoryUser {
                        id: 2,
                        name: "other-admin".to_string(),
                        enabled: true,
                        is_admin: true,
                        password_hash: crate::login::hash_password("other-password"),
                    },
                ])
                .keys(vec![MemoryUserKey {
                    id: 20,
                    user_id: 2,
                    api_key: "sk-other-admin".to_string(),
                    label: Some("other".to_string()),
                    enabled: true,
                }])
                .build(),
        );
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "admin".to_string(),
                password: crate::login::hash_password("old-password"),
                enabled: false,
                is_admin: false,
            })
            .await
            .expect("seed admin user");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 2,
                name: "other-admin".to_string(),
                password: crate::login::hash_password("other-password"),
                enabled: true,
                is_admin: true,
            })
            .await
            .expect("seed other admin user");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 20,
                user_id: 2,
                api_key: "sk-other-admin".to_string(),
                label: Some("other".to_string()),
                enabled: true,
            })
            .await
            .expect("seed other admin key");

        reconcile_bootstrap_admin(
            &state,
            &BootstrapAdmin {
                username: "admin".to_string(),
                password: Some("new-password".to_string()),
                api_key: Some("sk-admin-new".to_string()),
            },
            false,
        )
        .await
        .expect("reconcile bootstrap admin");

        let users = storage
            .list_users(&gproxy_storage::UserQuery::default())
            .await
            .expect("query users");
        assert_eq!(users.len(), 2);
        assert!(
            users
                .iter()
                .any(|user| user.name == "admin" && user.is_admin && user.enabled)
        );
        assert!(
            users
                .iter()
                .any(|user| user.name == "other-admin" && user.is_admin && user.enabled)
        );

        let admin = users
            .iter()
            .find(|user| user.name == "admin")
            .expect("admin");
        let admin_keys = storage
            .list_user_keys(&UserKeyQuery {
                user_id: Scope::Eq(admin.id),
                ..Default::default()
            })
            .await
            .expect("query admin keys");
        assert_eq!(admin_keys.len(), 1);
        assert_eq!(admin_keys[0].api_key, "sk-admin-new");
        assert!(admin_keys[0].enabled);
    }
}
