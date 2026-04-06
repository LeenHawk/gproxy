//! Bootstrap logic shared by the startup path and admin reload endpoint.

use std::collections::HashMap;

use gproxy_sdk::provider::engine::{GproxyEngineBuilder, ProviderConfig};
use gproxy_server::{
    AppState, FilePermissionEntry, GlobalConfig, MemoryClaudeFile, MemoryModel, MemoryUser,
    MemoryUserCredentialFile, MemoryUserKey, ModelAliasTarget, PermissionEntry, PriceTier,
    RateLimitRule,
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

struct SeedProviderRuntimeState {
    provider_configs: Vec<ProviderConfig>,
    provider_name_to_id: HashMap<String, i64>,
    provider_channel_map: HashMap<String, String>,
    provider_credentials: HashMap<String, Vec<i64>>,
    credential_positions: HashMap<i64, (String, usize)>,
}

fn synthetic_provider_id(index: usize) -> i64 {
    index as i64 + 1
}

fn synthetic_credential_id(provider_id: i64, index: usize) -> i64 {
    provider_id * 1000 + index as i64
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
            match gproxy_sdk::provider::engine::validate_credential_json(channel, credential) {
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
            match gproxy_sdk::provider::engine::validate_credential_json(
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
        });
        provider_channel_map.insert(provider.name.clone(), provider.channel.clone());

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
    let replacement_config = storage.get_global_settings().await?.map(|settings| GlobalConfig {
        host: settings.host,
        port: settings.port as u16,
        admin_key: settings.admin_key,
        proxy: settings.proxy,
        spoof_emulation: settings.spoof_emulation.unwrap_or_default(),
        update_source: settings.update_source.unwrap_or_default(),
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
    let aliases = storage
        .list_model_aliases(&gproxy_storage::ModelAliasQuery::default())
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
    let provider_name_map: HashMap<String, i64> =
        providers.iter().map(|provider| (provider.name.clone(), provider.id)).collect();
    let provider_channel_map: HashMap<String, String> = providers
        .iter()
        .map(|provider| (provider.name.clone(), provider.channel.clone()))
        .collect();

    let user_count = users.len();
    let memory_users: Vec<MemoryUser> = users
        .iter()
        .map(|user| MemoryUser {
            id: user.id,
            name: user.name.clone(),
            enabled: user.enabled,
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
    let memory_models: Vec<MemoryModel> = models
        .iter()
        .map(|model| {
            let price_tiers: Vec<PriceTier> = model
                .price_tiers_json
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_default();
            MemoryModel {
                id: model.id,
                provider_id: model.provider_id,
                model_id: model.model_id.clone(),
                display_name: model.display_name.clone(),
                enabled: model.enabled,
                price_each_call: model.price_each_call,
                price_tiers,
            }
        })
        .collect();

    let alias_count = aliases.len();
    let provider_name_by_id: HashMap<i64, String> = providers
        .iter()
        .map(|provider| (provider.id, provider.name.clone()))
        .collect();
    let alias_map: HashMap<String, ModelAliasTarget> = aliases
        .into_iter()
        .filter(|alias| alias.enabled)
        .map(|alias| {
            let provider_name = provider_name_by_id
                .get(&alias.provider_id)
                .cloned()
                .unwrap_or_else(|| alias.provider_id.to_string());
            (
                alias.alias,
                ModelAliasTarget {
                    provider_name,
                    model_id: alias.model_id,
                },
            )
        })
        .collect();

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
        limit_map.entry(limit.user_id).or_default().push(RateLimitRule {
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
    state.replace_users(memory_users);
    state.replace_keys(memory_keys);
    state.replace_models(memory_models);
    state.replace_model_aliases(alias_map);
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
        aliases: alias_count,
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config: GproxyToml = toml::from_str(toml_str)?;

    // 1. Global settings → memory + DB
    if let Some(gs) = &config.global {
        let gc = GlobalConfig {
            host: gs.host.clone(),
            port: gs.port,
            admin_key: gs.admin_key.clone(),
            proxy: gs.proxy.clone(),
            spoof_emulation: gs.spoof_emulation.clone(),
            update_source: gs.update_source.clone(),
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
                admin_key: gc.admin_key.clone(),
                proxy: gc.proxy.clone(),
                spoof_emulation: gc.spoof_emulation.clone(),
                update_source: gc.update_source.clone(),
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
                admin_key: cfg.admin_key.clone(),
                proxy: cfg.proxy.clone(),
                spoof_emulation: cfg.spoof_emulation.clone(),
                update_source: cfg.update_source.clone(),
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
                settings_json: serde_json::to_string(&p.settings).unwrap_or_default(),
                dispatch_json: String::new(),
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
    state.replace_engine(builder.build());
    apply_persisted_credential_statuses(state, &provider_runtime.credential_positions).await?;

    state.replace_provider_names(provider_runtime.provider_name_to_id.clone());
    state.replace_provider_channels(provider_runtime.provider_channel_map);
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
            })
            .await?;
        state.upsert_user_in_memory(MemoryUser {
            id: user_id,
            name: u.name.clone(),
            enabled: u.enabled,
            password_hash: hashed_password.clone(),
        });
        for (j, key) in u.keys.iter().enumerate() {
            let key_id = user_id * 1000 + j as i64;
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

    // 4. Models → memory + DB
    let models: Vec<MemoryModel> = config
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let provider_id = provider_runtime
                .provider_name_to_id
                .get(&m.provider_name)
                .copied()
                .unwrap_or(0);
            MemoryModel {
                id: i as i64 + 1,
                provider_id,
                model_id: m.model_id.clone(),
                display_name: m.display_name.clone(),
                enabled: m.enabled,
                price_each_call: m.price_each_call,
                price_tiers: m.price_tiers.clone(),
            }
        })
        .collect();
    for m in &models {
        state
            .storage()
            .upsert_model(gproxy_storage::ModelWrite {
                id: m.id,
                provider_id: m.provider_id,
                model_id: m.model_id.clone(),
                display_name: m.display_name.clone(),
                enabled: m.enabled,
                price_each_call: m.price_each_call,
                price_tiers_json: if m.price_tiers.is_empty() {
                    None
                } else {
                    serde_json::to_string(&m.price_tiers).ok()
                },
            })
            .await?;
    }
    state.replace_models(models);

    // 5. Model aliases → memory + DB
    let mut alias_map = HashMap::new();
    for (i, a) in config.model_aliases.iter().enumerate() {
        if !a.enabled {
            continue;
        }
        let provider_id = provider_runtime
            .provider_name_to_id
            .get(&a.provider_name)
            .copied()
            .unwrap_or(0);
        state
            .storage()
            .upsert_model_alias(gproxy_storage::ModelAliasWrite {
                id: i as i64 + 1,
                alias: a.alias.clone(),
                provider_id,
                model_id: a.model_id.clone(),
                enabled: true,
            })
            .await?;
        alias_map.insert(
            a.alias.clone(),
            ModelAliasTarget {
                provider_name: a.provider_name.clone(),
                model_id: a.model_id.clone(),
            },
        );
    }
    state.replace_model_aliases(alias_map);

    // 6. Permissions, file permissions, rate limits, quotas → memory + DB
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

    Ok(())
}

/// Seed the database with minimal defaults (global_settings only).
pub async fn seed_defaults(
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cfg = state.config().clone();
    state
        .storage()
        .upsert_global_settings(gproxy_storage::GlobalSettingsWrite {
            host: cfg.host.clone(),
            port: cfg.port,
            admin_key: cfg.admin_key.clone(),
            proxy: cfg.proxy.clone(),
            spoof_emulation: cfg.spoof_emulation.clone(),
            update_source: cfg.update_source.clone(),
            enable_usage: cfg.enable_usage,
            enable_upstream_log: cfg.enable_upstream_log,
            enable_upstream_log_body: cfg.enable_upstream_log_body,
            enable_downstream_log: cfg.enable_downstream_log,
            enable_downstream_log_body: cfg.enable_downstream_log_body,
            dsn: cfg.dsn.clone(),
            data_dir: cfg.data_dir.clone(),
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sea_orm::ConnectionTrait;
    use serde_json::json;

    use super::{build_seed_provider_runtime_state, reload_from_db};
    use crate::admin::config_toml::ProviderToml;
    use gproxy_server::{AppStateBuilder, GlobalConfig, MemoryUser, MemoryUserKey};
    use gproxy_storage::{
        GlobalSettingsWrite, SeaOrmStorage, SettingsRepository, UserRepository,
    };

    #[test]
    fn seed_provider_runtime_matches_reload_shape() {
        let state = build_seed_provider_runtime_state(&[
            ProviderToml {
                name: "first".to_string(),
                channel: "anthropic".to_string(),
                settings: json!({"region": "us"}),
                credentials: vec![json!({"api_key": "key-1"})],
            },
            ProviderToml {
                name: "second".to_string(),
                channel: "claudecode".to_string(),
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
            .engine(gproxy_sdk::provider::engine::GproxyEngine::builder().build())
            .storage(storage.clone())
            .config(GlobalConfig {
                admin_key: "memory-admin".to_string(),
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .users(vec![MemoryUser {
                id: 9,
                name: "memory-user".to_string(),
                enabled: true,
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
                update_source: "github".to_string(),
                admin_key: "db-admin".to_string(),
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

        assert_eq!(state.config().admin_key, "memory-admin");
        assert_eq!(state.config().proxy, None);

        let users = state.users_snapshot();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].id, 9);
        assert_eq!(users[0].name, "memory-user");

        let keys = state.keys_for_user(9);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, 99);
        assert_eq!(keys[0].api_key, "memory-key");
    }
}
