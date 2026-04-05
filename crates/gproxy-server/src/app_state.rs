use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use dashmap::DashMap;

use gproxy_sdk::provider::engine::GproxyEngine;
use gproxy_storage::SeaOrmStorage;

use crate::config::GlobalConfig;
use crate::middleware::model_alias::ModelAliasTarget;
use crate::middleware::permission::{self, FilePermissionEntry, PermissionEntry};
use crate::middleware::rate_limit::{
    RateLimitCounters, RateLimitRejection, RateLimitRule, find_matching_rule,
};
use crate::principal::{MemoryUser, MemoryUserKey};
pub use gproxy_core::{MemoryClaudeFile, MemoryModel, MemoryUserCredentialFile, PriceTier};

// Re-export middleware types
pub use crate::middleware::model_alias::ModelAliasTarget as ModelAliasTargetExport;
pub use crate::middleware::permission::FilePermissionEntry as FilePermissionEntryExport;
pub use crate::middleware::permission::PermissionEntry as PermissionEntryExport;
pub use crate::middleware::rate_limit::{
    RateLimitRejection as RateLimitRejectionExport, RateLimitRule as RateLimitRuleExport,
};

/// Central application state shared across all request handlers.
pub struct AppState {
    engine: ArcSwap<GproxyEngine>,
    storage: Arc<ArcSwap<SeaOrmStorage>>,
    config: ArcSwap<GlobalConfig>,
    users: ArcSwap<Vec<MemoryUser>>,
    keys: ArcSwap<HashMap<String, MemoryUserKey>>,
    models: ArcSwap<Vec<MemoryModel>>,
    model_aliases: ArcSwap<HashMap<String, ModelAliasTarget>>,
    provider_names: ArcSwap<HashMap<String, i64>>,
    provider_channels: ArcSwap<HashMap<String, String>>,
    provider_credentials: ArcSwap<HashMap<String, Vec<i64>>>,
    user_files: ArcSwap<Vec<MemoryUserCredentialFile>>,
    claude_files: ArcSwap<HashMap<(i64, String), MemoryClaudeFile>>,
    user_permissions: ArcSwap<HashMap<i64, Vec<PermissionEntry>>>,
    user_file_permissions: ArcSwap<HashMap<i64, Vec<FilePermissionEntry>>>,
    user_rate_limits: ArcSwap<HashMap<i64, Vec<RateLimitRule>>>,
    user_quotas: DashMap<i64, (f64, f64)>,
    pub rate_counters: RateLimitCounters,
    /// Optional async usage sink for non-blocking data plane writes.
    /// When set, `record_usage` sends through this channel instead of
    /// synchronous DB writes.
    usage_tx: Option<tokio::sync::mpsc::Sender<gproxy_storage::UsageWrite>>,
    /// Quota backend for pre-hold/settle pattern.
    pub quota_backend: gproxy_sdk::provider::InMemoryQuota,
}

impl AppState {
    // -----------------------------------------------------------------------
    // Read
    // -----------------------------------------------------------------------

    pub fn engine(&self) -> Arc<GproxyEngine> {
        self.engine.load_full()
    }

    pub fn storage(&self) -> Arc<SeaOrmStorage> {
        self.storage.load_full()
    }

    pub fn config(&self) -> Arc<GlobalConfig> {
        self.config.load_full()
    }

    /// Get the async usage sink sender, if configured.
    pub fn usage_tx(&self) -> Option<&tokio::sync::mpsc::Sender<gproxy_storage::UsageWrite>> {
        self.usage_tx.as_ref()
    }

    pub fn authenticate_api_key(&self, api_key: &str) -> Option<MemoryUserKey> {
        let keys = self.keys.load();
        let key = keys.get(api_key)?;
        if !key.enabled {
            return None;
        }
        let users = self.users.load();
        if !users.iter().any(|u| u.id == key.user_id && u.enabled) {
            return None;
        }
        Some(key.clone())
    }

    /// Get all keys for a user (from memory).
    pub fn keys_for_user(&self, user_id: i64) -> Vec<MemoryUserKey> {
        let keys = self.keys.load();
        keys.values()
            .filter(|k| k.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Get all users (from memory).
    pub fn users_snapshot(&self) -> Arc<Vec<MemoryUser>> {
        self.users.load_full()
    }

    /// Get all keys (from memory).
    pub fn keys_snapshot(&self) -> Arc<HashMap<String, MemoryUserKey>> {
        self.keys.load_full()
    }

    pub fn find_model(&self, model_id: &str) -> Option<MemoryModel> {
        self.models
            .load()
            .iter()
            .find(|m| m.model_id == model_id && m.enabled)
            .cloned()
    }

    /// Get user quota info: (quota, cost_used). Returns (0, 0) if not set.
    pub fn get_user_quota(&self, user_id: i64) -> (f64, f64) {
        self.user_quotas
            .get(&user_id)
            .map(|e| *e.value())
            .unwrap_or((0.0, 0.0))
    }

    pub fn resolve_model_alias(&self, alias: &str) -> Option<ModelAliasTarget> {
        self.model_aliases.load().get(alias).cloned()
    }

    pub fn check_model_permission(&self, user_id: i64, provider_name: &str, model: &str) -> bool {
        let provider_id = self
            .provider_names
            .load()
            .get(provider_name)
            .copied()
            .unwrap_or(0);
        let perms = self.user_permissions.load();
        let Some(entries) = perms.get(&user_id) else {
            return false;
        };
        entries.iter().any(|e| {
            let provider_ok = e.provider_id.is_none() || e.provider_id == Some(provider_id);
            provider_ok && permission::pattern_matches(&e.model_pattern, model)
        })
    }

    pub fn check_provider_access(&self, user_id: i64, provider_name: &str) -> bool {
        let Some(provider_id) = self.provider_names.load().get(provider_name).copied() else {
            return false;
        };
        let perms = self.user_permissions.load();
        let Some(entries) = perms.get(&user_id) else {
            return false;
        };
        entries
            .iter()
            .any(|e| e.provider_id.is_none() || e.provider_id == Some(provider_id))
    }

    pub fn check_file_permission(&self, user_id: i64, provider_name: &str) -> bool {
        let Some(provider_id) = self.provider_names.load().get(provider_name).copied() else {
            return false;
        };
        let perms = self.user_file_permissions.load();
        let Some(entries) = perms.get(&user_id) else {
            return false;
        };
        entries.iter().any(|entry| entry.provider_id == provider_id)
    }

    pub fn check_rate_limit(&self, user_id: i64, model: &str) -> Result<(), RateLimitRejection> {
        self.check_rate_limit_request(user_id, model, None)
    }

    pub fn check_rate_limit_request(
        &self,
        user_id: i64,
        model: &str,
        requested_total_tokens: Option<i64>,
    ) -> Result<(), RateLimitRejection> {
        let limits = self.user_rate_limits.load();
        let Some(user_limits) = limits.get(&user_id) else {
            return Ok(());
        };
        let Some(rule) = find_matching_rule(user_limits, model) else {
            return Ok(());
        };

        if let (Some(limit), Some(requested)) = (rule.total_tokens, requested_total_tokens)
            && requested > limit
        {
            return Err(RateLimitRejection::TotalTokens { limit, requested });
        }

        self.rate_counters
            .try_acquire(user_id, model, rule.rpm, rule.rpd)?;

        self.check_quota(user_id)?;
        Ok(())
    }

    /// Check if user has remaining cost quota. Separated from rate limiting
    /// to allow future replacement with QuotaBackend pre-hold model.
    pub fn check_quota(&self, user_id: i64) -> Result<(), RateLimitRejection> {
        let (quota, cost_used) = self.get_user_quota(user_id);
        if quota > 0.0 && cost_used >= quota {
            return Err(RateLimitRejection::QuotaExhausted { quota, cost_used });
        }
        Ok(())
    }

    /// Sync a user's quota into the QuotaBackend for pre-hold support.
    pub fn sync_quota_to_backend(&self, user_id: i64) {
        let (quota, _cost_used) = self.get_user_quota(user_id);
        if quota > 0.0 {
            // Set total quota in backend (cost_used tracking is separate)
            #[allow(clippy::let_underscore_future)]
            let _ = std::pin::pin!(
                gproxy_sdk::provider::QuotaBackend::set_quota(
                    &self.quota_backend,
                    user_id,
                    (quota * 1_000_000.0) as u64, // Convert to micro-units for integer precision
                )
            );
        }
    }

    /// Atomically add cost to a user's quota usage. Returns (quota, new_cost_used).
    pub fn add_cost_usage(&self, user_id: i64, cost: f64) -> (f64, f64) {
        let mut entry = self.user_quotas.entry(user_id).or_insert((0.0, 0.0));
        entry.1 += cost;
        *entry.value()
    }

    pub fn upsert_user_quota_in_memory(&self, user_id: i64, quota: f64, cost_used: f64) {
        self.user_quotas.insert(user_id, (quota, cost_used));
    }

    pub fn provider_id_for_name(&self, provider_name: &str) -> Option<i64> {
        self.provider_names.load().get(provider_name).copied()
    }

    pub fn provider_channel_for_name(&self, provider_name: &str) -> Option<String> {
        self.provider_channels.load().get(provider_name).cloned()
    }

    pub fn credential_id_for_index(&self, provider_name: &str, index: usize) -> Option<i64> {
        self.provider_credentials
            .load()
            .get(provider_name)
            .and_then(|ids| ids.get(index))
            .copied()
    }

    pub fn provider_credential_ids_for(&self, provider_name: &str) -> Option<Vec<i64>> {
        self.provider_credentials.load().get(provider_name).cloned()
    }

    pub fn credential_position_for_id(&self, credential_id: i64) -> Option<(String, usize)> {
        let provider_credentials = self.provider_credentials.load();
        provider_credentials
            .iter()
            .find_map(|(provider_name, ids)| {
                ids.iter()
                    .position(|id| *id == credential_id)
                    .map(|index| (provider_name.clone(), index))
            })
    }

    pub fn find_user_file(
        &self,
        user_id: i64,
        provider_name: &str,
        file_id: &str,
    ) -> Option<MemoryUserCredentialFile> {
        let provider_id = self.provider_id_for_name(provider_name)?;
        self.user_files
            .load()
            .iter()
            .find(|record| {
                record.active
                    && record.user_id == user_id
                    && record.provider_id == provider_id
                    && record.file_id == file_id
            })
            .cloned()
    }

    pub fn list_user_files(
        &self,
        user_id: i64,
        provider_name: &str,
    ) -> Vec<MemoryUserCredentialFile> {
        let Some(provider_id) = self.provider_id_for_name(provider_name) else {
            return Vec::new();
        };
        self.user_files
            .load()
            .iter()
            .filter(|record| {
                record.active && record.user_id == user_id && record.provider_id == provider_id
            })
            .cloned()
            .collect()
    }

    pub fn find_claude_file(&self, provider_id: i64, file_id: &str) -> Option<MemoryClaudeFile> {
        self.claude_files
            .load()
            .get(&(provider_id, file_id.to_string()))
            .cloned()
    }

    // -----------------------------------------------------------------------
    // Write
    // -----------------------------------------------------------------------

    pub fn replace_engine(&self, engine: GproxyEngine) {
        self.engine.store(Arc::new(engine));
    }

    pub fn replace_engine_arc(&self, engine: Arc<GproxyEngine>) {
        self.engine.store(engine);
    }

    pub fn replace_storage(&self, storage: SeaOrmStorage) {
        self.storage.store(Arc::new(storage));
    }

    pub fn replace_config(&self, config: GlobalConfig) {
        self.config.store(Arc::new(config));
    }

    pub fn upsert_user_in_memory(&self, user: MemoryUser) {
        let mut users = (*self.users.load_full()).clone();
        if let Some(existing) = users.iter_mut().find(|u| u.id == user.id) {
            *existing = user;
        } else {
            users.push(user);
        }
        self.users.store(Arc::new(users));
    }

    pub fn remove_user_from_memory(&self, user_id: i64) {
        let mut users = (*self.users.load_full()).clone();
        users.retain(|u| u.id != user_id);
        self.users.store(Arc::new(users));
        let mut keys = (*self.keys.load_full()).clone();
        keys.retain(|_, k| k.user_id != user_id);
        self.keys.store(Arc::new(keys));
        self.remove_file_permissions_for_user(user_id);
        self.remove_user_files_for_user(user_id);
    }

    pub fn upsert_key_in_memory(&self, key: MemoryUserKey) {
        let mut keys = (*self.keys.load_full()).clone();
        keys.retain(|_, k| k.id != key.id);
        keys.insert(key.api_key.clone(), key);
        self.keys.store(Arc::new(keys));
    }

    pub fn remove_key_from_memory(&self, key_id: i64) {
        let mut keys = (*self.keys.load_full()).clone();
        keys.retain(|_, k| k.id != key_id);
        self.keys.store(Arc::new(keys));
    }

    pub fn replace_users(&self, users: Vec<MemoryUser>) {
        self.users.store(Arc::new(users));
    }

    pub fn replace_keys(&self, keys: Vec<MemoryUserKey>) {
        let map: HashMap<String, MemoryUserKey> =
            keys.into_iter().map(|k| (k.api_key.clone(), k)).collect();
        self.keys.store(Arc::new(map));
    }

    pub fn replace_models(&self, models: Vec<MemoryModel>) {
        self.models.store(Arc::new(models));
    }

    pub fn replace_model_aliases(&self, aliases: HashMap<String, ModelAliasTarget>) {
        self.model_aliases.store(Arc::new(aliases));
    }

    pub fn replace_provider_names(&self, names: HashMap<String, i64>) {
        self.provider_names.store(Arc::new(names));
    }

    pub fn replace_provider_channels(&self, channels: HashMap<String, String>) {
        self.provider_channels.store(Arc::new(channels));
    }

    pub fn upsert_provider_name_in_memory(&self, name: String, provider_id: i64) {
        let mut names = (*self.provider_names.load_full()).clone();
        names.insert(name, provider_id);
        self.provider_names.store(Arc::new(names));
    }

    pub fn upsert_provider_channel_in_memory(&self, name: String, channel: String) {
        let mut channels = (*self.provider_channels.load_full()).clone();
        channels.insert(name, channel);
        self.provider_channels.store(Arc::new(channels));
    }

    pub fn remove_provider_name_from_memory(&self, name: &str) {
        let mut names = (*self.provider_names.load_full()).clone();
        names.remove(name);
        self.provider_names.store(Arc::new(names));
    }

    pub fn remove_provider_channel_from_memory(&self, name: &str) {
        let mut channels = (*self.provider_channels.load_full()).clone();
        channels.remove(name);
        self.provider_channels.store(Arc::new(channels));
    }

    pub fn replace_provider_credentials(&self, map: HashMap<String, Vec<i64>>) {
        self.provider_credentials.store(Arc::new(map));
    }

    pub fn replace_provider_credential_ids_in_memory(&self, name: String, ids: Vec<i64>) {
        let mut map = (*self.provider_credentials.load_full()).clone();
        map.insert(name, ids);
        self.provider_credentials.store(Arc::new(map));
    }

    pub fn append_provider_credential_id_in_memory(&self, name: &str, credential_id: i64) {
        let mut map = (*self.provider_credentials.load_full()).clone();
        map.entry(name.to_string()).or_default().push(credential_id);
        self.provider_credentials.store(Arc::new(map));
    }

    pub fn remove_provider_credential_index_in_memory(&self, name: &str, index: usize) {
        let mut map = (*self.provider_credentials.load_full()).clone();
        if let Some(ids) = map.get_mut(name) {
            if index < ids.len() {
                ids.remove(index);
            }
            if ids.is_empty() {
                map.remove(name);
            }
        }
        self.provider_credentials.store(Arc::new(map));
    }

    pub fn remove_provider_credentials_from_memory(&self, name: &str) {
        let mut map = (*self.provider_credentials.load_full()).clone();
        map.remove(name);
        self.provider_credentials.store(Arc::new(map));
    }

    pub fn replace_user_files(&self, files: Vec<MemoryUserCredentialFile>) {
        self.user_files.store(Arc::new(files));
    }

    pub fn upsert_user_file_in_memory(&self, file: MemoryUserCredentialFile) {
        let mut files = (*self.user_files.load_full()).clone();
        if let Some(existing) = files.iter_mut().find(|existing| {
            existing.user_id == file.user_id
                && existing.provider_id == file.provider_id
                && existing.file_id == file.file_id
        }) {
            *existing = file;
        } else {
            files.push(file);
        }
        self.user_files.store(Arc::new(files));
    }

    pub fn deactivate_user_file_in_memory(&self, user_id: i64, provider_id: i64, file_id: &str) {
        let mut files = (*self.user_files.load_full()).clone();
        if let Some(existing) = files.iter_mut().find(|existing| {
            existing.user_id == user_id
                && existing.provider_id == provider_id
                && existing.file_id == file_id
        }) {
            existing.active = false;
        }
        self.user_files.store(Arc::new(files));
    }

    pub fn remove_user_files_for_user(&self, user_id: i64) {
        let mut files = (*self.user_files.load_full()).clone();
        files.retain(|file| file.user_id != user_id);
        self.user_files.store(Arc::new(files));
    }

    pub fn remove_user_files_for_provider(&self, provider_id: i64) {
        let mut files = (*self.user_files.load_full()).clone();
        files.retain(|file| file.provider_id != provider_id);
        self.user_files.store(Arc::new(files));
        let mut claude_files = (*self.claude_files.load_full()).clone();
        claude_files.retain(|(current_provider_id, _), _| *current_provider_id != provider_id);
        self.claude_files.store(Arc::new(claude_files));
    }

    pub fn remove_user_files_for_credential(&self, credential_id: i64) {
        let mut files = (*self.user_files.load_full()).clone();
        files.retain(|file| file.credential_id != credential_id);
        self.user_files.store(Arc::new(files));
    }

    pub fn replace_claude_files(&self, files: HashMap<(i64, String), MemoryClaudeFile>) {
        self.claude_files.store(Arc::new(files));
    }

    pub fn upsert_claude_file_in_memory(&self, file: MemoryClaudeFile) {
        let mut files = (*self.claude_files.load_full()).clone();
        files.insert((file.provider_id, file.file_id.clone()), file);
        self.claude_files.store(Arc::new(files));
    }

    pub fn replace_user_permissions(&self, perms: HashMap<i64, Vec<PermissionEntry>>) {
        let normalized = perms
            .into_iter()
            .filter_map(|(user_id, entries)| {
                let mut normalized_entries: Vec<PermissionEntry> = Vec::new();
                for entry in entries {
                    if let Some(existing) = normalized_entries.iter_mut().find(|existing| {
                        existing.provider_id == entry.provider_id
                            && existing.model_pattern == entry.model_pattern
                    }) {
                        if entry.id < existing.id {
                            *existing = entry;
                        }
                    } else {
                        normalized_entries.push(entry);
                    }
                }
                (!normalized_entries.is_empty()).then_some((user_id, normalized_entries))
            })
            .collect();
        self.user_permissions.store(Arc::new(normalized));
    }

    pub fn replace_user_file_permissions(&self, perms: HashMap<i64, Vec<FilePermissionEntry>>) {
        let normalized = perms
            .into_iter()
            .filter_map(|(user_id, entries)| {
                let mut normalized_entries: Vec<FilePermissionEntry> = Vec::new();
                for entry in entries {
                    if let Some(existing) = normalized_entries
                        .iter_mut()
                        .find(|existing| existing.provider_id == entry.provider_id)
                    {
                        if entry.id < existing.id {
                            *existing = entry;
                        }
                    } else {
                        normalized_entries.push(entry);
                    }
                }
                (!normalized_entries.is_empty()).then_some((user_id, normalized_entries))
            })
            .collect();
        self.user_file_permissions.store(Arc::new(normalized));
    }

    pub fn replace_user_rate_limits(&self, limits: HashMap<i64, Vec<RateLimitRule>>) {
        self.user_rate_limits.store(Arc::new(limits));
    }

    pub fn user_quotas_snapshot(&self) -> HashMap<i64, (f64, f64)> {
        self.user_quotas
            .iter()
            .map(|e| (*e.key(), *e.value()))
            .collect()
    }

    pub fn replace_user_quotas(&self, quotas: HashMap<i64, (f64, f64)>) {
        self.user_quotas.clear();
        for (k, v) in quotas {
            self.user_quotas.insert(k, v);
        }
    }

    // --- Models ---

    pub fn models(&self) -> Arc<Vec<MemoryModel>> {
        self.models.load_full()
    }

    pub fn upsert_model_in_memory(&self, model: MemoryModel) {
        let mut models = (*self.models.load_full()).clone();
        if let Some(existing) = models.iter_mut().find(|m| m.id == model.id) {
            *existing = model;
        } else {
            models.push(model);
        }
        self.models.store(Arc::new(models));
    }

    pub fn remove_model_from_memory(&self, model_id: i64) {
        let mut models = (*self.models.load_full()).clone();
        models.retain(|m| m.id != model_id);
        self.models.store(Arc::new(models));
    }

    // --- Model aliases ---

    pub fn model_aliases_snapshot(&self) -> Arc<HashMap<String, ModelAliasTarget>> {
        self.model_aliases.load_full()
    }

    pub fn upsert_model_alias_in_memory(&self, alias: String, target: ModelAliasTarget) {
        let mut aliases = (*self.model_aliases.load_full()).clone();
        aliases.insert(alias, target);
        self.model_aliases.store(Arc::new(aliases));
    }

    pub fn remove_model_alias_from_memory(&self, alias: &str) {
        let mut aliases = (*self.model_aliases.load_full()).clone();
        aliases.remove(alias);
        self.model_aliases.store(Arc::new(aliases));
    }

    // --- User permissions ---

    pub fn user_permissions_snapshot(&self) -> Arc<HashMap<i64, Vec<PermissionEntry>>> {
        self.user_permissions.load_full()
    }

    pub fn upsert_permission_in_memory(&self, user_id: i64, entry: PermissionEntry) {
        let mut perms = (*self.user_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|existing| existing.id != entry.id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        let entries = perms.entry(user_id).or_default();
        if let Some(existing) = entries.iter_mut().find(|existing| {
            existing.provider_id == entry.provider_id
                && existing.model_pattern == entry.model_pattern
        }) {
            *existing = entry;
        } else {
            entries.push(entry);
        }
        self.user_permissions.store(Arc::new(perms));
    }

    pub fn remove_permission_from_memory(&self, permission_id: i64) {
        let mut perms = (*self.user_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|entry| entry.id != permission_id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        self.user_permissions.store(Arc::new(perms));
    }

    // --- User file permissions ---

    pub fn user_file_permissions_snapshot(&self) -> Arc<HashMap<i64, Vec<FilePermissionEntry>>> {
        self.user_file_permissions.load_full()
    }

    pub fn upsert_file_permission_in_memory(&self, user_id: i64, entry: FilePermissionEntry) {
        let mut perms = (*self.user_file_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|existing| existing.id != entry.id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        let entries = perms.entry(user_id).or_default();
        if let Some(existing) = entries
            .iter_mut()
            .find(|existing| existing.provider_id == entry.provider_id)
        {
            *existing = entry;
        } else {
            entries.push(entry);
        }
        self.user_file_permissions.store(Arc::new(perms));
    }

    pub fn remove_file_permission_from_memory(&self, permission_id: i64) {
        let mut perms = (*self.user_file_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|entry| entry.id != permission_id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        self.user_file_permissions.store(Arc::new(perms));
    }

    pub fn remove_file_permissions_for_user(&self, user_id: i64) {
        let mut perms = (*self.user_file_permissions.load_full()).clone();
        perms.remove(&user_id);
        self.user_file_permissions.store(Arc::new(perms));
    }

    pub fn remove_file_permissions_for_provider(&self, provider_id: i64) {
        let mut perms = (*self.user_file_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|entry| entry.provider_id != provider_id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        self.user_file_permissions.store(Arc::new(perms));
    }

    // --- User rate limits ---

    pub fn user_rate_limits_snapshot(&self) -> Arc<HashMap<i64, Vec<RateLimitRule>>> {
        self.user_rate_limits.load_full()
    }

    pub fn upsert_rate_limit_in_memory(&self, user_id: i64, rule: RateLimitRule) {
        let mut limits = (*self.user_rate_limits.load_full()).clone();
        let rules = limits.entry(user_id).or_default();
        if let Some(existing) = rules
            .iter_mut()
            .find(|r| r.model_pattern == rule.model_pattern)
        {
            *existing = rule;
        } else {
            rules.push(rule);
        }
        self.user_rate_limits.store(Arc::new(limits));
    }

    pub fn remove_rate_limit_from_memory(&self, user_id: i64, model_pattern: &str) {
        let mut limits = (*self.user_rate_limits.load_full()).clone();
        if let Some(rules) = limits.get_mut(&user_id) {
            rules.retain(|r| r.model_pattern != model_pattern);
            if rules.is_empty() {
                limits.remove(&user_id);
            }
        }
        self.user_rate_limits.store(Arc::new(limits));
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct AppStateBuilder {
    engine: Option<GproxyEngine>,
    storage: Option<Arc<SeaOrmStorage>>,
    config: Option<GlobalConfig>,
    users: Vec<MemoryUser>,
    keys: Vec<MemoryUserKey>,
    usage_tx: Option<tokio::sync::mpsc::Sender<gproxy_storage::UsageWrite>>,
}

impl AppStateBuilder {
    pub fn new() -> Self {
        Self {
            engine: None,
            storage: None,
            config: None,
            users: Vec::new(),
            keys: Vec::new(),
            usage_tx: None,
        }
    }

    pub fn engine(mut self, engine: GproxyEngine) -> Self {
        self.engine = Some(engine);
        self
    }

    pub fn storage(mut self, storage: Arc<SeaOrmStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn config(mut self, config: GlobalConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn users(mut self, users: Vec<MemoryUser>) -> Self {
        self.users = users;
        self
    }

    pub fn keys(mut self, keys: Vec<MemoryUserKey>) -> Self {
        self.keys = keys;
        self
    }

    /// Set the async usage sink sender for non-blocking data plane writes.
    pub fn usage_tx(mut self, tx: tokio::sync::mpsc::Sender<gproxy_storage::UsageWrite>) -> Self {
        self.usage_tx = Some(tx);
        self
    }

    pub fn build(self) -> AppState {
        let config = self.config.unwrap_or_default();
        let key_map: HashMap<String, MemoryUserKey> = self
            .keys
            .into_iter()
            .map(|k| (k.api_key.clone(), k))
            .collect();

        AppState {
            engine: ArcSwap::from_pointee(self.engine.expect("GproxyEngine is required")),
            storage: Arc::new(ArcSwap::from_pointee(
                (*self.storage.expect("SeaOrmStorage is required")).clone(),
            )),
            config: ArcSwap::from_pointee(config.clone()),
            users: ArcSwap::from_pointee(self.users),
            keys: ArcSwap::from_pointee(key_map),
            models: ArcSwap::from_pointee(Vec::new()),
            model_aliases: ArcSwap::from_pointee(HashMap::new()),
            provider_names: ArcSwap::from_pointee(HashMap::new()),
            provider_channels: ArcSwap::from_pointee(HashMap::new()),
            provider_credentials: ArcSwap::from_pointee(HashMap::new()),
            user_files: ArcSwap::from_pointee(Vec::new()),
            claude_files: ArcSwap::from_pointee(HashMap::new()),
            user_permissions: ArcSwap::from_pointee(HashMap::new()),
            user_file_permissions: ArcSwap::from_pointee(HashMap::new()),
            user_rate_limits: ArcSwap::from_pointee(HashMap::new()),
            user_quotas: DashMap::new(),
            rate_counters: RateLimitCounters::new(),
            usage_tx: self.usage_tx,
            quota_backend: gproxy_sdk::provider::InMemoryQuota::new(),
        }
    }
}

impl Default for AppStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}
