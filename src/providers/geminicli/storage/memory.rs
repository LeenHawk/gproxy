use anyhow::Result;
use async_trait::async_trait;

use crate::providers::credential_index;
use crate::providers::geminicli::{GeminiCliBackend, GeminiCliCredential, GeminiCliSetting};
use crate::storage::{ConfigStore, MemoryStorage};

#[async_trait]
impl GeminiCliBackend for MemoryStorage {
    async fn get_config(&self) -> Result<GeminiCliSetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.geminicli.setting)
    }

    async fn load_config(&self) -> Result<GeminiCliSetting> {
        self.get_config().await
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut GeminiCliSetting) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        update(&mut config.providers.geminicli.setting);
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<GeminiCliCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.geminicli.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<GeminiCliCredential>> {
        self.get_credentials().await
    }

    async fn add_credential(&self, credential: GeminiCliCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.geminicli;
        credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
        let key = credential.project_id.trim().to_string();
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
        F: FnOnce(&mut GeminiCliCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.geminicli;
        credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
        let Some(credential) = provider.credentials.get_mut(index) else {
            return Ok(());
        };
        let old_key = credential.project_id.clone();
        update(credential);
        let new_key = credential.project_id.clone();
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
        F: FnOnce(&mut GeminiCliCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.geminicli;
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
        let old_key = credential.project_id.clone();
        update(credential);
        let new_key = credential.project_id.clone();
        credential_index::update_index_on_change(
            &mut provider.credential_index,
            &old_key,
            &new_key,
            index,
        );
        self.save_app_config(&config).await
    }

    async fn delete_credential(&self, project_id: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.geminicli;
        provider
            .credentials
            .retain(|cred| cred.project_id != project_id);
        provider.rebuild_credential_index();
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<GeminiCliCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.geminicli.credentials.get(index).cloned())
    }
}
