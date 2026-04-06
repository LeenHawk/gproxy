use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use dashmap::DashMap;

use gproxy_core::{ConfigService, FileService, IdentityService, PolicyService, RoutingService};
use gproxy_sdk::provider::engine::GproxyEngine;
use gproxy_storage::SeaOrmStorage;

use crate::config::GlobalConfig;
use crate::middleware::model_alias::ModelAliasTarget;
use crate::middleware::permission::{FilePermissionEntry, PermissionEntry};
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

struct RoutingMirror {
    model_aliases: ArcSwap<HashMap<String, ModelAliasTarget>>,
    provider_names: ArcSwap<HashMap<String, i64>>,
    provider_channels: ArcSwap<HashMap<String, String>>,
    provider_credentials: ArcSwap<HashMap<String, Vec<i64>>>,
}

impl RoutingMirror {
    fn new() -> Self {
        Self {
            model_aliases: ArcSwap::from_pointee(HashMap::new()),
            provider_names: ArcSwap::from_pointee(HashMap::new()),
            provider_channels: ArcSwap::from_pointee(HashMap::new()),
            provider_credentials: ArcSwap::from_pointee(HashMap::new()),
        }
    }
}

struct FileMirror {
    user_files: ArcSwap<Vec<MemoryUserCredentialFile>>,
    claude_files: ArcSwap<HashMap<(i64, String), MemoryClaudeFile>>,
}

impl FileMirror {
    fn new() -> Self {
        Self {
            user_files: ArcSwap::from_pointee(Vec::new()),
            claude_files: ArcSwap::from_pointee(HashMap::new()),
        }
    }
}

struct PolicyMirror {
    user_permissions: ArcSwap<HashMap<i64, Vec<PermissionEntry>>>,
    user_file_permissions: ArcSwap<HashMap<i64, Vec<FilePermissionEntry>>>,
    user_rate_limits: ArcSwap<HashMap<i64, Vec<RateLimitRule>>>,
}

impl PolicyMirror {
    fn new() -> Self {
        Self {
            user_permissions: ArcSwap::from_pointee(HashMap::new()),
            user_file_permissions: ArcSwap::from_pointee(HashMap::new()),
            user_rate_limits: ArcSwap::from_pointee(HashMap::new()),
        }
    }
}

fn normalize_permissions(
    perms: HashMap<i64, Vec<PermissionEntry>>,
) -> HashMap<i64, Vec<PermissionEntry>> {
    perms
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
        .collect()
}

fn normalize_file_permissions(
    perms: HashMap<i64, Vec<FilePermissionEntry>>,
) -> HashMap<i64, Vec<FilePermissionEntry>> {
    perms
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
        .collect()
}

/// Central application state shared across all request handlers.
pub struct AppState {
    engine: ArcSwap<GproxyEngine>,
    storage: Arc<ArcSwap<SeaOrmStorage>>,
    pub identity: IdentityService,
    pub policy: PolicyService,
    pub routing: RoutingService,
    pub file: FileService,
    pub config_service: ConfigService,
    routing_mirror: RoutingMirror,
    file_mirror: FileMirror,
    policy_mirror: PolicyMirror,
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
    fn store_model_aliases(&self, aliases: HashMap<String, ModelAliasTarget>) {
        self.routing.replace_model_aliases(aliases.clone());
        self.routing_mirror.model_aliases.store(Arc::new(aliases));
    }

    fn store_provider_names(&self, names: HashMap<String, i64>) {
        self.routing.replace_provider_names(names.clone());
        self.routing_mirror.provider_names.store(Arc::new(names));
    }

    fn store_provider_channels(&self, channels: HashMap<String, String>) {
        self.routing.replace_provider_channels(channels.clone());
        self.routing_mirror
            .provider_channels
            .store(Arc::new(channels));
    }

    fn store_provider_credentials(&self, map: HashMap<String, Vec<i64>>) {
        self.routing.replace_provider_credentials(map.clone());
        self.routing_mirror
            .provider_credentials
            .store(Arc::new(map));
    }

    fn store_user_files(&self, files: Vec<MemoryUserCredentialFile>) {
        self.file.replace_user_files(files.clone());
        self.file_mirror.user_files.store(Arc::new(files));
    }

    fn store_claude_files(&self, files: HashMap<(i64, String), MemoryClaudeFile>) {
        self.file.replace_claude_files(files.clone());
        self.file_mirror.claude_files.store(Arc::new(files));
    }

    fn store_user_permissions(&self, perms: HashMap<i64, Vec<PermissionEntry>>) {
        self.policy.replace_permissions(perms.clone());
        self.policy_mirror.user_permissions.store(Arc::new(perms));
    }

    fn store_user_file_permissions(&self, perms: HashMap<i64, Vec<FilePermissionEntry>>) {
        self.policy.replace_file_permissions(perms.clone());
        self.policy_mirror
            .user_file_permissions
            .store(Arc::new(perms));
    }

    fn store_user_rate_limits(&self, limits: HashMap<i64, Vec<RateLimitRule>>) {
        self.policy.replace_rate_limits(limits.clone());
        self.policy_mirror.user_rate_limits.store(Arc::new(limits));
    }

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
        self.config_service.get()
    }

    /// Get the async usage sink sender, if configured.
    pub fn usage_tx(&self) -> Option<&tokio::sync::mpsc::Sender<gproxy_storage::UsageWrite>> {
        self.usage_tx.as_ref()
    }

    pub fn authenticate_api_key(&self, api_key: &str) -> Option<MemoryUserKey> {
        self.identity.authenticate_api_key(api_key)
    }

    /// Get all keys for a user (from memory).
    pub fn keys_for_user(&self, user_id: i64) -> Vec<MemoryUserKey> {
        self.identity.keys_for_user(user_id)
    }

    /// Get all users (from memory).
    pub fn users_snapshot(&self) -> Arc<Vec<MemoryUser>> {
        self.identity.users_snapshot()
    }

    /// Get all keys (from memory).
    pub fn keys_snapshot(&self) -> Arc<HashMap<String, MemoryUserKey>> {
        self.identity.keys_snapshot()
    }

    pub fn find_model(&self, model_id: &str) -> Option<MemoryModel> {
        self.routing.find_model(model_id)
    }

    /// Get user quota info: (quota, cost_used). Returns (0, 0) if not set.
    pub fn get_user_quota(&self, user_id: i64) -> (f64, f64) {
        self.user_quotas
            .get(&user_id)
            .map(|e| *e.value())
            .unwrap_or((0.0, 0.0))
    }

    pub fn resolve_model_alias(&self, alias: &str) -> Option<ModelAliasTarget> {
        self.routing.resolve_model_alias(alias)
    }

    pub fn check_model_permission(&self, user_id: i64, provider_name: &str, model: &str) -> bool {
        let provider_id = self.routing.provider_id_for_name(provider_name).unwrap_or(0);
        self.policy
            .check_model_permission(user_id, provider_id, model)
    }

    pub fn check_provider_access(&self, user_id: i64, provider_name: &str) -> bool {
        let Some(provider_id) = self.routing.provider_id_for_name(provider_name) else {
            return false;
        };
        self.policy.check_provider_access(user_id, provider_id)
    }

    pub fn check_file_permission(&self, user_id: i64, provider_name: &str) -> bool {
        let Some(provider_id) = self.routing.provider_id_for_name(provider_name) else {
            return false;
        };
        self.policy.check_file_permission(user_id, provider_id)
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
        let limits = self.policy_mirror.user_rate_limits.load();
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
        use std::task::Context;
        let (quota, _cost_used) = self.get_user_quota(user_id);
        if quota > 0.0 {
            let micro_units = (quota * 1_000_000.0) as u64;
            // InMemoryQuota::set_quota returns Ready — poll once to execute.
            let mut fut = std::pin::pin!(
                gproxy_sdk::provider::QuotaBackend::set_quota(
                    &self.quota_backend,
                    user_id,
                    micro_units,
                )
            );
            let waker = futures_util::task::noop_waker();
            let mut cx = Context::from_waker(&waker);
            let _ = fut.as_mut().poll(&mut cx);
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
        self.routing.provider_id_for_name(provider_name)
    }

    pub fn provider_channel_for_name(&self, provider_name: &str) -> Option<String> {
        self.routing.provider_channel_for_name(provider_name)
    }

    pub fn credential_id_for_index(&self, provider_name: &str, index: usize) -> Option<i64> {
        self.routing.credential_id_for_index(provider_name, index)
    }

    pub fn provider_credential_ids_for(&self, provider_name: &str) -> Option<Vec<i64>> {
        self.routing.provider_credential_ids(provider_name)
    }

    pub fn credential_position_for_id(&self, credential_id: i64) -> Option<(String, usize)> {
        self.routing.credential_position_for_id(credential_id)
    }

    pub fn find_user_file(
        &self,
        user_id: i64,
        provider_name: &str,
        file_id: &str,
    ) -> Option<MemoryUserCredentialFile> {
        let provider_id = self.routing.provider_id_for_name(provider_name)?;
        self.file.find_user_file(user_id, provider_id, file_id)
    }

    pub fn list_user_files(
        &self,
        user_id: i64,
        provider_name: &str,
    ) -> Vec<MemoryUserCredentialFile> {
        let Some(provider_id) = self.routing.provider_id_for_name(provider_name) else {
            return Vec::new();
        };
        self.file.list_user_files(user_id, provider_id)
    }

    pub fn find_claude_file(&self, provider_id: i64, file_id: &str) -> Option<MemoryClaudeFile> {
        self.file.find_claude_file(provider_id, file_id)
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
        self.config_service.replace(config);
    }

    pub fn upsert_user_in_memory(&self, user: MemoryUser) {
        self.identity.upsert_user(user);
    }

    pub fn remove_user_from_memory(&self, user_id: i64) {
        self.identity.remove_user(user_id);
        self.remove_file_permissions_for_user(user_id);
        self.remove_user_files_for_user(user_id);
    }

    pub fn upsert_key_in_memory(&self, key: MemoryUserKey) {
        self.identity.upsert_key(key);
    }

    pub fn remove_key_from_memory(&self, key_id: i64) {
        self.identity.remove_key(key_id);
    }

    pub fn replace_users(&self, users: Vec<MemoryUser>) {
        self.identity.replace_users(users);
    }

    pub fn replace_keys(&self, keys: Vec<MemoryUserKey>) {
        self.identity.replace_keys(keys);
    }

    pub fn replace_models(&self, models: Vec<MemoryModel>) {
        self.routing.replace_models(models);
    }

    pub fn replace_model_aliases(&self, aliases: HashMap<String, ModelAliasTarget>) {
        self.store_model_aliases(aliases);
    }

    pub fn replace_provider_names(&self, names: HashMap<String, i64>) {
        self.store_provider_names(names);
    }

    pub fn replace_provider_channels(&self, channels: HashMap<String, String>) {
        self.store_provider_channels(channels);
    }

    pub fn upsert_provider_name_in_memory(&self, name: String, provider_id: i64) {
        let mut names = (*self.routing_mirror.provider_names.load_full()).clone();
        names.insert(name, provider_id);
        self.store_provider_names(names);
    }

    pub fn upsert_provider_channel_in_memory(&self, name: String, channel: String) {
        let mut channels = (*self.routing_mirror.provider_channels.load_full()).clone();
        channels.insert(name, channel);
        self.store_provider_channels(channels);
    }

    pub fn remove_provider_name_from_memory(&self, name: &str) {
        let mut names = (*self.routing_mirror.provider_names.load_full()).clone();
        names.remove(name);
        self.store_provider_names(names);
    }

    pub fn remove_provider_channel_from_memory(&self, name: &str) {
        let mut channels = (*self.routing_mirror.provider_channels.load_full()).clone();
        channels.remove(name);
        self.store_provider_channels(channels);
    }

    pub fn replace_provider_credentials(&self, map: HashMap<String, Vec<i64>>) {
        self.store_provider_credentials(map);
    }

    pub fn replace_provider_credential_ids_in_memory(&self, name: String, ids: Vec<i64>) {
        let mut map = (*self.routing_mirror.provider_credentials.load_full()).clone();
        map.insert(name, ids);
        self.store_provider_credentials(map);
    }

    pub fn append_provider_credential_id_in_memory(&self, name: &str, credential_id: i64) {
        let mut map = (*self.routing_mirror.provider_credentials.load_full()).clone();
        map.entry(name.to_string()).or_default().push(credential_id);
        self.store_provider_credentials(map);
    }

    pub fn remove_provider_credential_index_in_memory(&self, name: &str, index: usize) {
        let mut map = (*self.routing_mirror.provider_credentials.load_full()).clone();
        if let Some(ids) = map.get_mut(name) {
            if index < ids.len() {
                ids.remove(index);
            }
            if ids.is_empty() {
                map.remove(name);
            }
        }
        self.store_provider_credentials(map);
    }

    pub fn remove_provider_credentials_from_memory(&self, name: &str) {
        let mut map = (*self.routing_mirror.provider_credentials.load_full()).clone();
        map.remove(name);
        self.store_provider_credentials(map);
    }

    pub fn replace_user_files(&self, files: Vec<MemoryUserCredentialFile>) {
        self.store_user_files(files);
    }

    pub fn upsert_user_file_in_memory(&self, file: MemoryUserCredentialFile) {
        let mut files = (*self.file_mirror.user_files.load_full()).clone();
        if let Some(existing) = files.iter_mut().find(|existing| {
            existing.user_id == file.user_id
                && existing.provider_id == file.provider_id
                && existing.file_id == file.file_id
        }) {
            *existing = file;
        } else {
            files.push(file);
        }
        self.store_user_files(files);
    }

    pub fn deactivate_user_file_in_memory(&self, user_id: i64, provider_id: i64, file_id: &str) {
        let mut files = (*self.file_mirror.user_files.load_full()).clone();
        if let Some(existing) = files.iter_mut().find(|existing| {
            existing.user_id == user_id
                && existing.provider_id == provider_id
                && existing.file_id == file_id
        }) {
            existing.active = false;
        }
        self.store_user_files(files);
    }

    pub fn remove_user_files_for_user(&self, user_id: i64) {
        let mut files = (*self.file_mirror.user_files.load_full()).clone();
        files.retain(|file| file.user_id != user_id);
        self.store_user_files(files);
    }

    pub fn remove_user_files_for_provider(&self, provider_id: i64) {
        let mut files = (*self.file_mirror.user_files.load_full()).clone();
        files.retain(|file| file.provider_id != provider_id);
        let mut claude_files = (*self.file_mirror.claude_files.load_full()).clone();
        claude_files.retain(|(current_provider_id, _), _| *current_provider_id != provider_id);
        self.store_user_files(files);
        self.store_claude_files(claude_files);
    }

    pub fn remove_user_files_for_credential(&self, credential_id: i64) {
        let mut files = (*self.file_mirror.user_files.load_full()).clone();
        files.retain(|file| file.credential_id != credential_id);
        self.store_user_files(files);
    }

    pub fn replace_claude_files(&self, files: HashMap<(i64, String), MemoryClaudeFile>) {
        self.store_claude_files(files);
    }

    pub fn upsert_claude_file_in_memory(&self, file: MemoryClaudeFile) {
        let mut files = (*self.file_mirror.claude_files.load_full()).clone();
        files.insert((file.provider_id, file.file_id.clone()), file);
        self.store_claude_files(files);
    }

    pub fn replace_user_permissions(&self, perms: HashMap<i64, Vec<PermissionEntry>>) {
        self.store_user_permissions(normalize_permissions(perms));
    }

    pub fn replace_user_file_permissions(&self, perms: HashMap<i64, Vec<FilePermissionEntry>>) {
        self.store_user_file_permissions(normalize_file_permissions(perms));
    }

    pub fn replace_user_rate_limits(&self, limits: HashMap<i64, Vec<RateLimitRule>>) {
        self.store_user_rate_limits(limits);
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
        self.routing.models_snapshot()
    }

    pub fn upsert_model_in_memory(&self, model: MemoryModel) {
        let mut models = (*self.routing.models_snapshot()).clone();
        if let Some(existing) = models.iter_mut().find(|existing| existing.id == model.id) {
            *existing = model;
        } else {
            models.push(model);
        }
        self.routing.replace_models(models);
    }

    pub fn remove_model_from_memory(&self, model_id: i64) {
        let mut models = (*self.routing.models_snapshot()).clone();
        models.retain(|model| model.id != model_id);
        self.routing.replace_models(models);
    }

    // --- Model aliases ---

    pub fn model_aliases_snapshot(&self) -> Arc<HashMap<String, ModelAliasTarget>> {
        self.routing_mirror.model_aliases.load_full()
    }

    pub fn upsert_model_alias_in_memory(&self, alias: String, target: ModelAliasTarget) {
        let mut aliases = (*self.routing_mirror.model_aliases.load_full()).clone();
        aliases.insert(alias, target);
        self.store_model_aliases(aliases);
    }

    pub fn remove_model_alias_from_memory(&self, alias: &str) {
        let mut aliases = (*self.routing_mirror.model_aliases.load_full()).clone();
        aliases.remove(alias);
        self.store_model_aliases(aliases);
    }

    // --- User permissions ---

    pub fn user_permissions_snapshot(&self) -> Arc<HashMap<i64, Vec<PermissionEntry>>> {
        self.policy_mirror.user_permissions.load_full()
    }

    pub fn upsert_permission_in_memory(&self, user_id: i64, entry: PermissionEntry) {
        let mut perms = (*self.policy_mirror.user_permissions.load_full()).clone();
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
        self.store_user_permissions(perms);
    }

    pub fn remove_permission_from_memory(&self, permission_id: i64) {
        let mut perms = (*self.policy_mirror.user_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|entry| entry.id != permission_id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        self.store_user_permissions(perms);
    }

    // --- User file permissions ---

    pub fn user_file_permissions_snapshot(&self) -> Arc<HashMap<i64, Vec<FilePermissionEntry>>> {
        self.policy_mirror.user_file_permissions.load_full()
    }

    pub fn upsert_file_permission_in_memory(&self, user_id: i64, entry: FilePermissionEntry) {
        let mut perms = (*self.policy_mirror.user_file_permissions.load_full()).clone();
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
        self.store_user_file_permissions(perms);
    }

    pub fn remove_file_permission_from_memory(&self, permission_id: i64) {
        let mut perms = (*self.policy_mirror.user_file_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|entry| entry.id != permission_id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        self.store_user_file_permissions(perms);
    }

    pub fn remove_file_permissions_for_user(&self, user_id: i64) {
        let mut perms = (*self.policy_mirror.user_file_permissions.load_full()).clone();
        perms.remove(&user_id);
        self.store_user_file_permissions(perms);
    }

    pub fn remove_file_permissions_for_provider(&self, provider_id: i64) {
        let mut perms = (*self.policy_mirror.user_file_permissions.load_full()).clone();
        for entries in perms.values_mut() {
            entries.retain(|entry| entry.provider_id != provider_id);
        }
        perms.retain(|_, entries| !entries.is_empty());
        self.store_user_file_permissions(perms);
    }

    // --- User rate limits ---

    pub fn user_rate_limits_snapshot(&self) -> Arc<HashMap<i64, Vec<RateLimitRule>>> {
        self.policy_mirror.user_rate_limits.load_full()
    }

    pub fn upsert_rate_limit_in_memory(&self, user_id: i64, rule: RateLimitRule) {
        let mut limits = (*self.policy_mirror.user_rate_limits.load_full()).clone();
        let rules = limits.entry(user_id).or_default();
        if let Some(existing) = rules
            .iter_mut()
            .find(|existing| existing.model_pattern == rule.model_pattern)
        {
            *existing = rule;
        } else {
            rules.push(rule);
        }
        self.store_user_rate_limits(limits);
    }

    pub fn remove_rate_limit_from_memory(&self, user_id: i64, model_pattern: &str) {
        let mut limits = (*self.policy_mirror.user_rate_limits.load_full()).clone();
        if let Some(rules) = limits.get_mut(&user_id) {
            rules.retain(|rule| rule.model_pattern != model_pattern);
            if rules.is_empty() {
                limits.remove(&user_id);
            }
        }
        self.store_user_rate_limits(limits);
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
        let AppStateBuilder {
            engine,
            storage,
            config,
            users,
            keys,
            usage_tx,
        } = self;

        let config = config.unwrap_or_default();
        let state = AppState {
            engine: ArcSwap::from_pointee(engine.expect("GproxyEngine is required")),
            storage: Arc::new(ArcSwap::from_pointee(
                (*storage.expect("SeaOrmStorage is required")).clone(),
            )),
            identity: IdentityService::new(),
            policy: PolicyService::new(),
            routing: RoutingService::new(),
            file: FileService::new(),
            config_service: ConfigService::new(),
            routing_mirror: RoutingMirror::new(),
            file_mirror: FileMirror::new(),
            policy_mirror: PolicyMirror::new(),
            user_quotas: DashMap::new(),
            rate_counters: RateLimitCounters::new(),
            usage_tx,
            quota_backend: gproxy_sdk::provider::InMemoryQuota::new(),
        };

        state.replace_config(config);
        state.replace_users(users);
        state.replace_keys(keys);

        state
    }
}

impl Default for AppStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_sdk::protocol::claude::types::{FileMetadata, FileObjectType};

    async fn build_test_state() -> AppState {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );

        AppStateBuilder::new()
            .engine(GproxyEngine::builder().build())
            .storage(storage)
            .config(GlobalConfig {
                admin_key: "admin-key".to_string(),
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .users(vec![MemoryUser {
                id: 1,
                name: "alice".to_string(),
                enabled: true,
                password_hash: "hash".to_string(),
            }])
            .keys(vec![MemoryUserKey {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            }])
            .build()
    }

    #[tokio::test]
    async fn builder_seeds_identity_and_config_services() {
        let state = build_test_state().await;

        assert_eq!(state.config().admin_key, "admin-key");
        assert_eq!(state.config_service.get().admin_key, "admin-key");

        assert_eq!(state.users_snapshot().len(), 1);
        assert_eq!(state.identity.users_snapshot().len(), 1);

        assert_eq!(state.keys_for_user(1).len(), 1);
        assert_eq!(state.identity.keys_for_user(1).len(), 1);

        assert!(state.authenticate_api_key("sk-test").is_some());
        assert!(state.identity.authenticate_api_key("sk-test").is_some());
    }

    #[tokio::test]
    async fn app_state_methods_delegate_to_domain_services() {
        let state = build_test_state().await;

        state.replace_models(vec![MemoryModel {
            id: 100,
            provider_id: 42,
            model_id: "claude-3-5-sonnet".to_string(),
            display_name: Some("Claude 3.5 Sonnet".to_string()),
            enabled: true,
            price_each_call: None,
            price_tiers: Vec::new(),
        }]);
        state.replace_model_aliases(HashMap::from([(
            "sonnet".to_string(),
            ModelAliasTarget {
                provider_name: "anthropic".to_string(),
                model_id: "claude-3-5-sonnet".to_string(),
            },
        )]));
        state.replace_provider_names(HashMap::from([("anthropic".to_string(), 42)]));
        state.replace_provider_channels(HashMap::from([(
            "anthropic".to_string(),
            "claudecode".to_string(),
        )]));
        state.replace_provider_credentials(HashMap::from([(
            "anthropic".to_string(),
            vec![1000, 1001],
        )]));
        state.replace_user_permissions(HashMap::from([(
            1,
            vec![PermissionEntry {
                id: 1,
                provider_id: Some(42),
                model_pattern: "claude-*".to_string(),
            }],
        )]));
        state.replace_user_file_permissions(HashMap::from([(
            1,
            vec![FilePermissionEntry {
                id: 2,
                provider_id: 42,
            }],
        )]));
        state.replace_user_rate_limits(HashMap::from([(
            1,
            vec![RateLimitRule {
                id: 3,
                model_pattern: "claude-*".to_string(),
                rpm: Some(5),
                rpd: None,
                total_tokens: Some(100),
            }],
        )]));
        state.replace_user_files(vec![MemoryUserCredentialFile {
            user_id: 1,
            user_key_id: 10,
            provider_id: 42,
            credential_id: 1000,
            file_id: "file-1".to_string(),
            active: true,
            created_at_unix_ms: 123,
        }]);
        state.replace_claude_files(HashMap::from([(
            (42, "file-1".to_string()),
            MemoryClaudeFile {
                provider_id: 42,
                file_id: "file-1".to_string(),
                file_created_at_unix_ms: 456,
                metadata: FileMetadata {
                    id: "file-1".to_string(),
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                    filename: "doc.txt".to_string(),
                    mime_type: "text/plain".to_string(),
                    size_bytes: 12,
                    type_: FileObjectType::File,
                    downloadable: Some(true),
                },
            },
        )]));

        assert_eq!(state.find_model("claude-3-5-sonnet").unwrap().id, 100);
        assert_eq!(state.routing.find_model("claude-3-5-sonnet").unwrap().id, 100);

        assert_eq!(
            state.resolve_model_alias("sonnet").unwrap().provider_name,
            "anthropic"
        );
        assert_eq!(
            state
                .routing
                .resolve_model_alias("sonnet")
                .unwrap()
                .provider_name,
            "anthropic"
        );

        assert_eq!(state.provider_id_for_name("anthropic"), Some(42));
        assert_eq!(state.routing.provider_id_for_name("anthropic"), Some(42));

        assert_eq!(
            state.provider_channel_for_name("anthropic").as_deref(),
            Some("claudecode")
        );
        assert_eq!(
            state
                .routing
                .provider_channel_for_name("anthropic")
                .as_deref(),
            Some("claudecode")
        );

        assert_eq!(state.credential_id_for_index("anthropic", 1), Some(1001));
        assert_eq!(state.routing.credential_id_for_index("anthropic", 1), Some(1001));

        assert_eq!(
            state.provider_credential_ids_for("anthropic"),
            Some(vec![1000, 1001])
        );
        assert_eq!(
            state.routing.provider_credential_ids("anthropic"),
            Some(vec![1000, 1001])
        );

        assert_eq!(
            state.credential_position_for_id(1001),
            Some(("anthropic".to_string(), 1))
        );
        assert_eq!(
            state.routing.credential_position_for_id(1001),
            Some(("anthropic".to_string(), 1))
        );

        assert!(state.check_model_permission(1, "anthropic", "claude-3-5-sonnet"));
        assert!(state
            .policy
            .check_model_permission(1, 42, "claude-3-5-sonnet"));

        assert!(state.check_provider_access(1, "anthropic"));
        assert!(state.policy.check_provider_access(1, 42));

        assert!(state.check_file_permission(1, "anthropic"));
        assert!(state.policy.check_file_permission(1, 42));

        assert!(state.find_user_file(1, "anthropic", "file-1").is_some());
        assert!(state.file.find_user_file(1, 42, "file-1").is_some());

        assert_eq!(state.list_user_files(1, "anthropic").len(), 1);
        assert_eq!(state.file.list_user_files(1, 42).len(), 1);

        assert_eq!(
            state.find_claude_file(42, "file-1").unwrap().metadata.filename,
            "doc.txt"
        );
        assert_eq!(
            state
                .file
                .find_claude_file(42, "file-1")
                .unwrap()
                .metadata
                .filename,
            "doc.txt"
        );

        assert!(matches!(
            state.check_rate_limit_request(1, "claude-3-5-sonnet", Some(101)),
            Err(RateLimitRejection::TotalTokens {
                limit: 100,
                requested: 101
            })
        ));
    }
}
