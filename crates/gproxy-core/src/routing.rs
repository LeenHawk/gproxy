//! Routing service: models, aliases, and provider index lookups.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::types::{MemoryModel, ModelAliasTarget};

/// Manages model registry, alias resolution, and provider index mappings.
pub struct RoutingService {
    models: ArcSwap<Vec<MemoryModel>>,
    model_aliases: ArcSwap<HashMap<String, ModelAliasTarget>>,
    provider_names: ArcSwap<HashMap<String, i64>>,
    provider_channels: ArcSwap<HashMap<String, String>>,
    provider_credentials: ArcSwap<HashMap<String, Vec<i64>>>,
    /// Serializes single-item write operations to prevent lost updates.
    write_lock: Mutex<()>,
}

impl RoutingService {
    /// Creates a new empty routing service.
    pub fn new() -> Self {
        Self {
            models: ArcSwap::from(Arc::new(Vec::new())),
            model_aliases: ArcSwap::from(Arc::new(HashMap::new())),
            provider_names: ArcSwap::from(Arc::new(HashMap::new())),
            provider_channels: ArcSwap::from(Arc::new(HashMap::new())),
            provider_credentials: ArcSwap::from(Arc::new(HashMap::new())),
            write_lock: Mutex::new(()),
        }
    }

    /// Resolve a model alias to its target (provider_name, model_id).
    pub fn resolve_model_alias(&self, alias: &str) -> Option<ModelAliasTarget> {
        self.model_aliases.load().get(alias).cloned()
    }

    /// Find an enabled model by model_id.
    pub fn find_model(&self, model_id: &str) -> Option<MemoryModel> {
        self.models
            .load()
            .iter()
            .find(|m| m.model_id == model_id && m.enabled)
            .cloned()
    }

    /// Get provider DB id by name.
    pub fn provider_id_for_name(&self, name: &str) -> Option<i64> {
        self.provider_names.load().get(name).copied()
    }

    /// Get provider channel type by name.
    pub fn provider_channel_for_name(&self, name: &str) -> Option<String> {
        self.provider_channels.load().get(name).cloned()
    }

    /// Get credential DB id by provider name and index.
    pub fn credential_id_for_index(&self, provider_name: &str, index: usize) -> Option<i64> {
        self.provider_credentials
            .load()
            .get(provider_name)
            .and_then(|ids| ids.get(index))
            .copied()
    }

    /// Get all credential IDs for a provider.
    pub fn provider_credential_ids(&self, provider_name: &str) -> Option<Vec<i64>> {
        self.provider_credentials
            .load()
            .get(provider_name)
            .cloned()
    }

    /// Find (provider_name, index) for a credential ID.
    pub fn credential_position_for_id(&self, credential_id: i64) -> Option<(String, usize)> {
        let creds = self.provider_credentials.load();
        creds.iter().find_map(|(name, ids)| {
            ids.iter()
                .position(|id| *id == credential_id)
                .map(|idx| (name.clone(), idx))
        })
    }

    /// Get all models snapshot.
    pub fn models_snapshot(&self) -> Arc<Vec<MemoryModel>> {
        self.models.load_full()
    }

    // -- Bulk replace (bootstrap / reload) --

    /// Replace all models atomically.
    pub fn replace_models(&self, models: Vec<MemoryModel>) {
        self.models.store(Arc::new(models));
    }

    /// Replace all model aliases atomically.
    pub fn replace_model_aliases(&self, aliases: HashMap<String, ModelAliasTarget>) {
        self.model_aliases.store(Arc::new(aliases));
    }

    /// Replace all provider name → id mappings.
    pub fn replace_provider_names(&self, names: HashMap<String, i64>) {
        self.provider_names.store(Arc::new(names));
    }

    /// Replace all provider name → channel type mappings.
    pub fn replace_provider_channels(&self, channels: HashMap<String, String>) {
        self.provider_channels.store(Arc::new(channels));
    }

    /// Replace all provider credential ID mappings.
    pub fn replace_provider_credentials(&self, map: HashMap<String, Vec<i64>>) {
        self.provider_credentials.store(Arc::new(map));
    }

    // -- Single-item CRUD --

    /// Upsert a provider name → id mapping.
    pub fn upsert_provider_name(&self, name: String, provider_id: i64) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut names = (*self.provider_names.load_full()).clone();
        names.insert(name, provider_id);
        self.provider_names.store(Arc::new(names));
    }

    /// Remove a provider name mapping.
    pub fn remove_provider_name(&self, name: &str) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut names = (*self.provider_names.load_full()).clone();
        names.remove(name);
        self.provider_names.store(Arc::new(names));
    }

    /// Upsert a provider channel mapping.
    pub fn upsert_provider_channel(&self, name: String, channel: String) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut channels = (*self.provider_channels.load_full()).clone();
        channels.insert(name, channel);
        self.provider_channels.store(Arc::new(channels));
    }

    /// Remove a provider channel mapping.
    pub fn remove_provider_channel(&self, name: &str) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut channels = (*self.provider_channels.load_full()).clone();
        channels.remove(name);
        self.provider_channels.store(Arc::new(channels));
    }

    /// Replace credential IDs for a single provider.
    pub fn replace_provider_credential_ids(&self, name: String, ids: Vec<i64>) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut map = (*self.provider_credentials.load_full()).clone();
        map.insert(name, ids);
        self.provider_credentials.store(Arc::new(map));
    }

    /// Append a credential ID to a provider's list.
    pub fn append_provider_credential_id(&self, name: &str, credential_id: i64) {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut map = (*self.provider_credentials.load_full()).clone();
        map.entry(name.to_string()).or_default().push(credential_id);
        self.provider_credentials.store(Arc::new(map));
    }
}

impl Default for RoutingService {
    fn default() -> Self {
        Self::new()
    }
}
