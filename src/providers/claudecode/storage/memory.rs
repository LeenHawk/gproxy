use anyhow::Result;
use async_trait::async_trait;

use crate::providers::credential_index;
use crate::providers::claudecode::{ClaudeCodeBackend, ClaudeCodeCredential, ClaudeCodeSetting};
use crate::storage::{ConfigStore, MemoryStorage};

#[async_trait]
impl ClaudeCodeBackend for MemoryStorage {
    async fn get_config(&self) -> Result<ClaudeCodeSetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.claudecode.setting)
    }

    async fn load_config(&self) -> Result<ClaudeCodeSetting> {
        self.get_config().await
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeCodeSetting) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        update(&mut config.providers.claudecode.setting);
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<ClaudeCodeCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.claudecode.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<ClaudeCodeCredential>> {
        self.get_credentials().await
    }

    async fn add_credential(&self, credential: ClaudeCodeCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.claudecode;
        credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
        let key = credential.refresh_token.trim().to_string();
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
        F: FnOnce(&mut ClaudeCodeCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.claudecode;
        credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
        let Some(credential) = provider.credentials.get_mut(index) else {
            return Ok(());
        };
        let old_key = credential.refresh_token.clone();
        update(credential);
        let new_key = credential.refresh_token.clone();
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
        F: FnOnce(&mut ClaudeCodeCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.claudecode;
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
        let old_key = credential.refresh_token.clone();
        update(credential);
        let new_key = credential.refresh_token.clone();
        credential_index::update_index_on_change(
            &mut provider.credential_index,
            &old_key,
            &new_key,
            index,
        );
        self.save_app_config(&config).await
    }

    async fn delete_credential(&self, refresh_token: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.claudecode;
        provider
            .credentials
            .retain(|cred| cred.refresh_token != refresh_token);
        provider.rebuild_credential_index();
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<ClaudeCodeCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.claudecode.credentials.get(index).cloned())
    }
}
