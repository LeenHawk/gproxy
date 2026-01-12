// Macro contract: provider_storage_modules! expands to cfg-gated backend modules:
// database/file/memory/s3 based on storage-* features.
crate::provider_storage_modules!();

use anyhow::Result;
use async_trait::async_trait;

use crate::providers::claude::{ClaudeCredential, ClaudeSetting};

pub struct ClaudeStorage<'a, S> {
    storage: &'a S,
}

#[async_trait]
pub trait ClaudeBackend: Send + Sync {
    async fn get_config(&self) -> Result<ClaudeSetting>;
    async fn load_config(&self) -> Result<ClaudeSetting>;
    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeSetting) + Send;

    async fn get_credentials(&self) -> Result<Vec<ClaudeCredential>>;
    async fn load_credentials(&self) -> Result<Vec<ClaudeCredential>>;
    async fn add_credential(&self, credential: ClaudeCredential) -> Result<()>;
    async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeCredential) + Send;
    async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeCredential) + Send;
    async fn delete_credential(&self, key: &str) -> Result<()>;
    async fn get_credential(&self, index: usize) -> Result<Option<ClaudeCredential>>;
}

impl<'a, S> ClaudeStorage<'a, S>
where
    S: ClaudeBackend,
{
    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }

    pub async fn get_config(&self) -> Result<ClaudeSetting> {
        self.storage.get_config().await
    }

    pub async fn load_config(&self) -> Result<ClaudeSetting> {
        self.storage.load_config().await
    }

    pub async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeSetting) + Send,
    {
        self.storage.update_config(update).await
    }

    pub async fn get_credentials(&self) -> Result<Vec<ClaudeCredential>> {
        self.storage.get_credentials().await
    }

    pub async fn load_credentials(&self) -> Result<Vec<ClaudeCredential>> {
        self.storage.load_credentials().await
    }

    pub async fn add_credential(&self, credential: ClaudeCredential) -> Result<()> {
        self.storage.add_credential(credential).await
    }

    pub async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeCredential) + Send,
    {
        self.storage.update_credential(index, update).await
    }

    pub async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeCredential) + Send,
    {
        self.storage.update_credential_by_id(id, update).await
    }

    pub async fn delete_credential(&self, key: &str) -> Result<()> {
        self.storage.delete_credential(key).await
    }

    pub async fn get_credential(&self, index: usize) -> Result<Option<ClaudeCredential>> {
        self.storage.get_credential(index).await
    }
}

// Macro contract: provider_storage_backend! expands to StorageService dispatch for all backend
// operations plus a default "storage not configured" error arm.
crate::provider_storage_backend!(ClaudeBackend, ClaudeSetting, ClaudeCredential);
