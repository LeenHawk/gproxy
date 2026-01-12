use anyhow::Result;
use async_trait::async_trait;

use crate::providers::credential_index;
use crate::providers::deepseek::{DeepSeekBackend, DeepSeekCredential, DeepSeekSetting};
use crate::storage::{ConfigStore, MemoryStorage};

#[async_trait]
impl DeepSeekBackend for MemoryStorage {
    async fn get_config(&self) -> Result<DeepSeekSetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.deepseek.setting)
    }

    async fn load_config(&self) -> Result<DeepSeekSetting> {
        self.get_config().await
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut DeepSeekSetting) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        update(&mut config.providers.deepseek.setting);
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<DeepSeekCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.deepseek.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<DeepSeekCredential>> {
        self.get_credentials().await
    }

    async fn add_credential(&self, credential: DeepSeekCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.deepseek;
        credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
        let key = credential.key.trim().to_string();
        provider.credentials.push(credential);
        if !key.is_empty() {
            provider
                .credential_index
                .insert(key, provider.credentials.len() - 1);
        }
        self.save_app_config(&config).await
    }

    async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut DeepSeekCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.deepseek;
        credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
        let Some(credential) = provider.credentials.get_mut(index) else {
            return Ok(());
        };
        let old_key = credential.key.clone();
        update(credential);
        let new_key = credential.key.clone();
        credential_index::update_index_on_change(
            &mut provider.credential_index,
            &old_key,
            &new_key,
            index,
        );
        self.save_app_config(&config).await
    }

    async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut DeepSeekCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.deepseek;
        let Some(index) = credential_index::find_or_rebuild(
            &mut provider.credential_index,
            &provider.credentials,
            id,
        ) else {
            return Ok(());
        };
        let Some(credential) = provider.credentials.get_mut(index) else {
            return Ok(());
        };
        let old_key = credential.key.clone();
        update(credential);
        let new_key = credential.key.clone();
        credential_index::update_index_on_change(
            &mut provider.credential_index,
            &old_key,
            &new_key,
            index,
        );
        self.save_app_config(&config).await
    }

    async fn delete_credential(&self, key: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.deepseek;
        provider.credentials.retain(|cred| cred.key != key);
        provider.rebuild_credential_index();
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<DeepSeekCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.deepseek.credentials.get(index).cloned())
    }
}
