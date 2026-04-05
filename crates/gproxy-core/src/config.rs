//! Config service: server-wide configuration management.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::types::GlobalConfig;

/// Manages server-wide configuration (host, port, admin_key, logging flags, etc.).
pub struct ConfigService {
    config: ArcSwap<GlobalConfig>,
}

impl ConfigService {
    /// Creates a new config service with default configuration.
    pub fn new() -> Self {
        Self {
            config: ArcSwap::from(Arc::new(GlobalConfig::default())),
        }
    }

    /// Get the current configuration snapshot.
    pub fn get(&self) -> Arc<GlobalConfig> {
        self.config.load_full()
    }

    /// Replace the configuration atomically.
    pub fn replace(&self, config: GlobalConfig) {
        self.config.store(Arc::new(config));
    }
}

impl Default for ConfigService {
    fn default() -> Self {
        Self::new()
    }
}
