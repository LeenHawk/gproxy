use anyhow::Result;
use async_trait::async_trait;

use crate::providers::credential_index;
use crate::providers::vertexexpress::{
    VertexExpressBackend, VertexExpressCredential, VertexExpressSetting,
};
use crate::storage::{ConfigStore, MemoryStorage};

#[async_trait]
impl VertexExpressBackend for MemoryStorage {
    async fn get_config(&self) -> Result<VertexExpressSetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.vertexexpress.setting)
    }

    async fn load_config(&self) -> Result<VertexExpressSetting> {
        self.get_config().await
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexExpressSetting) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        update(&mut config.providers.vertexexpress.setting);
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<VertexExpressCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.vertexexpress.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<VertexExpressCredential>> {
        self.get_credentials().await
    }

    async fn add_credential(&self, credential: VertexExpressCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.vertexexpress;
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
        F: FnOnce(&mut VertexExpressCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.vertexexpress;
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
        F: FnOnce(&mut VertexExpressCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let mut config = self.get_app_config().await?;
        let provider = &mut config.providers.vertexexpress;
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
        let provider = &mut config.providers.vertexexpress;
        provider.credentials.retain(|cred| cred.key != key);
        provider.rebuild_credential_index();
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<VertexExpressCredential>> {
        let config = self.get_app_config().await?;
        Ok(config
            .providers
            .vertexexpress
            .credentials
            .get(index)
            .cloned())
    }
}
