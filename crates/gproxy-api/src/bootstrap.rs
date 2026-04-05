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

    // Global settings
    if let Some(settings) = storage.get_global_settings().await? {
        state.replace_config(GlobalConfig {
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
    }
    let config = state.config().clone();

    // Providers + credentials → engine
    let providers = storage
        .list_providers(&gproxy_storage::ProviderQuery::default())
        .await?;
    let all_credentials = storage
        .list_credentials(&gproxy_storage::CredentialQuery::default())
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
    state.replace_engine(builder.build());

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
    apply_persisted_credential_statuses(state, &credential_positions).await?;
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
    state.replace_provider_credentials(provider_credentials);

    // Provider name → id map for permission checks
    let provider_name_map: HashMap<String, i64> =
        providers.iter().map(|p| (p.name.clone(), p.id)).collect();
    state.replace_provider_names(provider_name_map.clone());
    let provider_channel_map: HashMap<String, String> = providers
        .iter()
        .map(|p| (p.name.clone(), p.channel.clone()))
        .collect();
    state.replace_provider_channels(provider_channel_map);

    // Users — atomic replace to remove stale entries
    let users = storage
        .list_users(&gproxy_storage::UserQuery::default())
        .await?;
    let user_count = users.len();
    let memory_users: Vec<MemoryUser> = users
        .iter()
        .map(|u| MemoryUser {
            id: u.id,
            name: u.name.clone(),
            enabled: u.enabled,
            password_hash: u.password.clone(),
        })
        .collect();
    state.replace_users(memory_users);

    // User keys — atomic replace to remove stale entries
    let keys = storage.list_user_keys_for_memory().await?;
    let key_count = keys.len();
    let memory_keys: Vec<MemoryUserKey> = keys
        .iter()
        .map(|k| MemoryUserKey {
            id: k.id,
            user_id: k.user_id,
            api_key: k.api_key.clone(),
            label: k.label.clone(),
            enabled: k.enabled,
        })
        .collect();
    state.replace_keys(memory_keys);

    // Models
    let models = storage
        .list_models(&gproxy_storage::ModelQuery::default())
        .await?;
    let model_count = models.len();
    let memory_models: Vec<MemoryModel> = models
        .iter()
        .map(|m| {
            let price_tiers: Vec<PriceTier> = m
                .price_tiers_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            MemoryModel {
                id: m.id,
                provider_id: m.provider_id,
                model_id: m.model_id.clone(),
                display_name: m.display_name.clone(),
                enabled: m.enabled,
                price_each_call: m.price_each_call,
                price_tiers,
            }
        })
        .collect();
    state.replace_models(memory_models);

    // Model aliases
    let aliases = storage
        .list_model_aliases(&gproxy_storage::ModelAliasQuery::default())
        .await?;
    let alias_count = aliases.len();
    let provider_name_map: HashMap<i64, String> =
        providers.iter().map(|p| (p.id, p.name.clone())).collect();
    let alias_map = aliases
        .into_iter()
        .filter(|a| a.enabled)
        .map(|a| {
            let provider_name = provider_name_map
                .get(&a.provider_id)
                .cloned()
                .unwrap_or_else(|| a.provider_id.to_string());
            (
                a.alias,
                ModelAliasTarget {
                    provider_name,
                    model_id: a.model_id,
                },
            )
        })
        .collect();
    state.replace_model_aliases(alias_map);

    // Permissions
    let perms = storage
        .list_user_model_permissions(&gproxy_storage::UserModelPermissionQuery::default())
        .await?;
    let perm_count = perms.len();
    let mut perm_map: HashMap<i64, Vec<PermissionEntry>> = HashMap::new();
    for p in perms {
        perm_map
            .entry(p.user_id)
            .or_default()
            .push(PermissionEntry {
                id: p.id,
                provider_id: p.provider_id,
                model_pattern: p.model_pattern,
            });
    }
    state.replace_user_permissions(perm_map);

    // File permissions
    let file_permissions = storage
        .list_user_file_permissions(&gproxy_storage::UserFilePermissionQuery::default())
        .await?;
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
    state.replace_user_file_permissions(file_permission_map);

    // Rate limits
    let limits = storage
        .list_user_rate_limits(&gproxy_storage::UserRateLimitQuery::default())
        .await?;
    let limit_count = limits.len();
    let mut limit_map: HashMap<i64, Vec<RateLimitRule>> = HashMap::new();
    for l in limits {
        limit_map.entry(l.user_id).or_default().push(RateLimitRule {
            id: l.id,
            model_pattern: l.model_pattern,
            rpm: l.rpm,
            rpd: l.rpd,
            total_tokens: l.total_tokens,
        });
    }
    state.replace_user_rate_limits(limit_map);

    // Quotas
    let quotas = storage.list_user_quotas().await?;
    let quota_count = quotas.len();
    let quota_map: HashMap<i64, (f64, f64)> = quotas
        .into_iter()
        .map(|q| (q.user_id, (q.quota, q.cost_used)))
        .collect();
    state.replace_user_quotas(quota_map);

    // File ownership
    let user_files = storage
        .list_user_credential_files(&gproxy_storage::UserCredentialFileQuery::default())
        .await?;
    let user_file_count = user_files.len();
    state.replace_user_files(
        user_files
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
            .collect(),
    );

    // Claude file metadata
    let claude_files = storage
        .list_claude_files(&gproxy_storage::ClaudeFileQuery::default())
        .await?;
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
    use serde_json::json;

    use super::build_seed_provider_runtime_state;
    use crate::admin::config_toml::ProviderToml;

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
}
