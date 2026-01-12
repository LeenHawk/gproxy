use anyhow::Result;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use toml_edit::DocumentMut;

use crate::config::AppConfig;
use crate::providers::ProvidersConfig;
use crate::storage::ConfigStore;

pub struct MemoryStorage {
    config: ArcSwap<AppConfig>,
    lock: Mutex<()>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            config: ArcSwap::from_pointee(AppConfig::default()),
            lock: Mutex::new(()),
        }
    }

    pub async fn providers_get<R, F>(&self, read: F) -> Result<R>
    where
        F: FnOnce(&ProvidersConfig) -> R + Send,
    {
        let config = self.config.load_full();
        Ok(read(&config.providers))
    }

    pub async fn providers_update<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut ProvidersConfig, Option<&mut DocumentMut>) -> Result<()> + Send,
    {
        let _guard = self.lock.lock().await;
        let mut config = (*self.config.load_full()).clone();
        update(&mut config.providers, None)?;
        self.config.store(Arc::new(config));
        Ok(())
    }

    pub(crate) async fn lock_update(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.lock.lock().await
    }
}

#[async_trait]
impl ConfigStore for MemoryStorage {
    async fn get_app_config(&self) -> Result<AppConfig> {
        Ok((*self.config.load_full()).clone())
    }

    async fn load_app_config(&self) -> Result<AppConfig> {
        Ok((*self.config.load_full()).clone())
    }

    async fn save_app_config(&self, config: &AppConfig) -> Result<()> {
        self.config.store(Arc::new(config.clone()));
        Ok(())
    }

    async fn flush_app_config(&self) -> Result<()> {
        Ok(())
    }
}
