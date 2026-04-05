//! Identity service: user authentication and API key management.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::types::{MemoryUser, MemoryUserKey};

/// Manages user records and API keys for authentication.
pub struct IdentityService {
    users: ArcSwap<Vec<MemoryUser>>,
    keys: ArcSwap<HashMap<String, MemoryUserKey>>,
}

impl IdentityService {
    /// Creates a new empty identity service.
    pub fn new() -> Self {
        Self {
            users: ArcSwap::from(Arc::new(Vec::new())),
            keys: ArcSwap::from(Arc::new(HashMap::new())),
        }
    }

    /// Authenticate an API key. Returns the key record if valid and the owning user is enabled.
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

    /// Get all keys for a given user.
    pub fn keys_for_user(&self, user_id: i64) -> Vec<MemoryUserKey> {
        let keys = self.keys.load();
        keys.values()
            .filter(|k| k.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Get all users snapshot.
    pub fn users_snapshot(&self) -> Arc<Vec<MemoryUser>> {
        self.users.load_full()
    }

    /// Get all keys snapshot.
    pub fn keys_snapshot(&self) -> Arc<HashMap<String, MemoryUserKey>> {
        self.keys.load_full()
    }

    // -- Bulk replace (bootstrap / reload) --

    /// Replace all users atomically.
    pub fn replace_users(&self, users: Vec<MemoryUser>) {
        self.users.store(Arc::new(users));
    }

    /// Replace all keys atomically.
    pub fn replace_keys(&self, keys: Vec<MemoryUserKey>) {
        let map: HashMap<String, MemoryUserKey> =
            keys.into_iter().map(|k| (k.api_key.clone(), k)).collect();
        self.keys.store(Arc::new(map));
    }

    // -- Single-item CRUD --

    /// Upsert a user in memory.
    pub fn upsert_user(&self, user: MemoryUser) {
        let mut users = (*self.users.load_full()).clone();
        if let Some(existing) = users.iter_mut().find(|u| u.id == user.id) {
            *existing = user;
        } else {
            users.push(user);
        }
        self.users.store(Arc::new(users));
    }

    /// Remove a user and their keys from memory.
    pub fn remove_user(&self, user_id: i64) {
        let mut users = (*self.users.load_full()).clone();
        users.retain(|u| u.id != user_id);
        self.users.store(Arc::new(users));
        let mut keys = (*self.keys.load_full()).clone();
        keys.retain(|_, k| k.user_id != user_id);
        self.keys.store(Arc::new(keys));
    }

    /// Upsert a key in memory.
    pub fn upsert_key(&self, key: MemoryUserKey) {
        let mut keys = (*self.keys.load_full()).clone();
        keys.retain(|_, k| k.id != key.id);
        keys.insert(key.api_key.clone(), key);
        self.keys.store(Arc::new(keys));
    }

    /// Remove a key from memory by ID.
    pub fn remove_key(&self, key_id: i64) {
        let mut keys = (*self.keys.load_full()).clone();
        keys.retain(|_, k| k.id != key_id);
        self.keys.store(Arc::new(keys));
    }
}

impl Default for IdentityService {
    fn default() -> Self {
        Self::new()
    }
}
