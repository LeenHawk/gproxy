// Macro contract: provider_storage_modules! expands to cfg-gated backend modules:
// database/file/memory/s3 based on storage-* features.
crate::provider_storage_modules!();

use anyhow::Result;
use async_trait::async_trait;

use crate::providers::codex::{CodexCredential, CodexSetting};

pub struct CodexStorage<'a, S> {
    storage: &'a S,
}

#[async_trait]
pub trait CodexBackend: Send + Sync {
    async fn get_config(&self) -> Result<CodexSetting>;
    async fn load_config(&self) -> Result<CodexSetting>;
    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut CodexSetting) + Send;

    async fn get_credentials(&self) -> Result<Vec<CodexCredential>>;
    async fn load_credentials(&self) -> Result<Vec<CodexCredential>>;
    async fn add_credential(&self, credential: CodexCredential) -> Result<()>;
    async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut CodexCredential) + Send;
    async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut CodexCredential) + Send;
    async fn delete_credential(&self, refresh_token: &str) -> Result<()>;
    async fn get_credential(&self, index: usize) -> Result<Option<CodexCredential>>;
}

impl<'a, S> CodexStorage<'a, S>
where
    S: CodexBackend,
{
    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }

    pub async fn get_config(&self) -> Result<CodexSetting> {
        self.storage.get_config().await
    }

    pub async fn load_config(&self) -> Result<CodexSetting> {
        self.storage.load_config().await
    }

    pub async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut CodexSetting) + Send,
    {
        self.storage.update_config(update).await
    }

    pub async fn get_credentials(&self) -> Result<Vec<CodexCredential>> {
        self.storage.get_credentials().await
    }

    pub async fn load_credentials(&self) -> Result<Vec<CodexCredential>> {
        self.storage.load_credentials().await
    }

    pub async fn add_credential(&self, credential: CodexCredential) -> Result<()> {
        self.storage.add_credential(credential).await
    }

    pub async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut CodexCredential) + Send,
    {
        self.storage.update_credential(index, update).await
    }

    pub async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut CodexCredential) + Send,
    {
        self.storage.update_credential_by_id(id, update).await
    }

    pub async fn delete_credential(&self, refresh_token: &str) -> Result<()> {
        self.storage.delete_credential(refresh_token).await
    }

    pub async fn get_credential(&self, index: usize) -> Result<Option<CodexCredential>> {
        self.storage.get_credential(index).await
    }
}

// Macro contract: provider_storage_backend! expands to StorageService dispatch for all backend
// operations plus a default "storage not configured" error arm.
crate::provider_storage_backend!(CodexBackend, CodexSetting, CodexCredential);
