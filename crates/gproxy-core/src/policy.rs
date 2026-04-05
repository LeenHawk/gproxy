//! Policy service: permissions, file permissions, and rate limit rules.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::types::{FilePermissionEntry, PermissionEntry, RateLimitRule};

/// Manages model permissions, file permissions, and rate limit rules.
pub struct PolicyService {
    user_permissions: ArcSwap<HashMap<i64, Vec<PermissionEntry>>>,
    user_file_permissions: ArcSwap<HashMap<i64, Vec<FilePermissionEntry>>>,
    user_rate_limits: ArcSwap<HashMap<i64, Vec<RateLimitRule>>>,
}

impl PolicyService {
    /// Creates a new empty policy service.
    pub fn new() -> Self {
        Self {
            user_permissions: ArcSwap::from(Arc::new(HashMap::new())),
            user_file_permissions: ArcSwap::from(Arc::new(HashMap::new())),
            user_rate_limits: ArcSwap::from(Arc::new(HashMap::new())),
        }
    }

    /// Check if a user has permission to access a model on a given provider.
    ///
    /// `provider_id` should be resolved by the caller (from SDK ProviderRegistry).
    pub fn check_model_permission(
        &self,
        user_id: i64,
        provider_id: i64,
        model: &str,
    ) -> bool {
        let perms = self.user_permissions.load();
        let Some(entries) = perms.get(&user_id) else {
            return false;
        };
        entries.iter().any(|e| {
            let provider_ok = e.provider_id.is_none() || e.provider_id == Some(provider_id);
            provider_ok && pattern_matches(&e.model_pattern, model)
        })
    }

    /// Check if a user has access to any model on a given provider.
    pub fn check_provider_access(&self, user_id: i64, provider_id: i64) -> bool {
        let perms = self.user_permissions.load();
        let Some(entries) = perms.get(&user_id) else {
            return false;
        };
        entries
            .iter()
            .any(|e| e.provider_id.is_none() || e.provider_id == Some(provider_id))
    }

    /// Check if a user has file upload permission for a provider.
    pub fn check_file_permission(&self, user_id: i64, provider_id: i64) -> bool {
        let perms = self.user_file_permissions.load();
        let Some(entries) = perms.get(&user_id) else {
            return false;
        };
        entries.iter().any(|entry| entry.provider_id == provider_id)
    }

    /// Find the matching rate limit rule for a user and model.
    pub fn find_rate_limit_rule(&self, user_id: i64, model: &str) -> Option<RateLimitRule> {
        let limits = self.user_rate_limits.load();
        let user_limits = limits.get(&user_id)?;
        user_limits
            .iter()
            .find(|rule| pattern_matches(&rule.model_pattern, model))
            .cloned()
    }

    // -- Bulk replace (bootstrap / reload) --

    /// Replace all user permissions atomically.
    pub fn replace_permissions(&self, perms: HashMap<i64, Vec<PermissionEntry>>) {
        self.user_permissions.store(Arc::new(perms));
    }

    /// Replace all file permissions atomically.
    pub fn replace_file_permissions(&self, perms: HashMap<i64, Vec<FilePermissionEntry>>) {
        self.user_file_permissions.store(Arc::new(perms));
    }

    /// Replace all rate limit rules atomically.
    pub fn replace_rate_limits(&self, limits: HashMap<i64, Vec<RateLimitRule>>) {
        self.user_rate_limits.store(Arc::new(limits));
    }

    // -- Single-item CRUD --

    /// Upsert a permission entry for a user.
    pub fn upsert_permission(&self, user_id: i64, entry: PermissionEntry) {
        let mut perms = (*self.user_permissions.load_full()).clone();
        let user_perms = perms.entry(user_id).or_default();
        if let Some(existing) = user_perms.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            user_perms.push(entry);
        }
        self.user_permissions.store(Arc::new(perms));
    }

    /// Remove a permission entry by ID.
    pub fn remove_permission(&self, user_id: i64, permission_id: i64) {
        let mut perms = (*self.user_permissions.load_full()).clone();
        if let Some(user_perms) = perms.get_mut(&user_id) {
            user_perms.retain(|e| e.id != permission_id);
        }
        self.user_permissions.store(Arc::new(perms));
    }

    /// Upsert a file permission entry for a user.
    pub fn upsert_file_permission(&self, user_id: i64, entry: FilePermissionEntry) {
        let mut perms = (*self.user_file_permissions.load_full()).clone();
        let user_perms = perms.entry(user_id).or_default();
        if let Some(existing) = user_perms.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            user_perms.push(entry);
        }
        self.user_file_permissions.store(Arc::new(perms));
    }

    /// Remove a file permission entry by ID.
    pub fn remove_file_permission(&self, user_id: i64, permission_id: i64) {
        let mut perms = (*self.user_file_permissions.load_full()).clone();
        if let Some(user_perms) = perms.get_mut(&user_id) {
            user_perms.retain(|e| e.id != permission_id);
        }
        self.user_file_permissions.store(Arc::new(perms));
    }

    /// Remove all file permissions for a user.
    pub fn remove_file_permissions_for_user(&self, user_id: i64) {
        let mut perms = (*self.user_file_permissions.load_full()).clone();
        perms.remove(&user_id);
        self.user_file_permissions.store(Arc::new(perms));
    }

    /// Upsert a rate limit rule for a user.
    pub fn upsert_rate_limit(&self, user_id: i64, rule: RateLimitRule) {
        let mut limits = (*self.user_rate_limits.load_full()).clone();
        let user_limits = limits.entry(user_id).or_default();
        if let Some(existing) = user_limits.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule;
        } else {
            user_limits.push(rule);
        }
        self.user_rate_limits.store(Arc::new(limits));
    }

    /// Remove a rate limit rule by ID.
    pub fn remove_rate_limit(&self, user_id: i64, rule_id: i64) {
        let mut limits = (*self.user_rate_limits.load_full()).clone();
        if let Some(user_limits) = limits.get_mut(&user_id) {
            user_limits.retain(|r| r.id != rule_id);
        }
        self.user_rate_limits.store(Arc::new(limits));
    }
}

impl Default for PolicyService {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a glob-style pattern matches a model string.
///
/// Supports `*` as a wildcard matching any sequence of characters.
fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }
    pattern == value
}
