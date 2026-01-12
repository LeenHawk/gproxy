// Macro contract: provider_storage_modules! expands to cfg-gated backend modules:
// database/file/memory/s3 based on storage-* features.
crate::provider_storage_modules!();

use anyhow::Result;
use async_trait::async_trait;

use crate::providers::geminicli::{GeminiCliCredential, GeminiCliSetting};

pub struct GeminiCliStorage<'a, S> {
    storage: &'a S,
}

#[async_trait]
pub trait GeminiCliBackend: Send + Sync {
    async fn get_config(&self) -> Result<GeminiCliSetting>;
    async fn load_config(&self) -> Result<GeminiCliSetting>;
    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut GeminiCliSetting) + Send;

    async fn get_credentials(&self) -> Result<Vec<GeminiCliCredential>>;
    async fn load_credentials(&self) -> Result<Vec<GeminiCliCredential>>;
    async fn add_credential(&self, credential: GeminiCliCredential) -> Result<()>;
    async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut GeminiCliCredential) + Send;
    async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut GeminiCliCredential) + Send;
    async fn delete_credential(&self, project_id: &str) -> Result<()>;
    async fn get_credential(&self, index: usize) -> Result<Option<GeminiCliCredential>>;
}

impl<'a, S> GeminiCliStorage<'a, S>
where
    S: GeminiCliBackend,
{
    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }

    pub async fn get_config(&self) -> Result<GeminiCliSetting> {
        self.storage.get_config().await
    }

    pub async fn load_config(&self) -> Result<GeminiCliSetting> {
        self.storage.load_config().await
    }

    pub async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut GeminiCliSetting) + Send,
    {
        self.storage.update_config(update).await
    }

    pub async fn get_credentials(&self) -> Result<Vec<GeminiCliCredential>> {
        self.storage.get_credentials().await
    }

    pub async fn load_credentials(&self) -> Result<Vec<GeminiCliCredential>> {
        self.storage.load_credentials().await
    }

    pub async fn add_credential(&self, credential: GeminiCliCredential) -> Result<()> {
        self.storage.add_credential(credential).await
    }

    pub async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut GeminiCliCredential) + Send,
    {
        self.storage.update_credential(index, update).await
    }

    pub async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut GeminiCliCredential) + Send,
    {
        self.storage.update_credential_by_id(id, update).await
    }

    pub async fn delete_credential(&self, project_id: &str) -> Result<()> {
        self.storage.delete_credential(project_id).await
    }

    pub async fn get_credential(&self, index: usize) -> Result<Option<GeminiCliCredential>> {
        self.storage.get_credential(index).await
    }
}

// Macro contract: provider_storage_backend! expands to StorageService dispatch for all backend
// operations plus a default "storage not configured" error arm.
crate::provider_storage_backend!(GeminiCliBackend, GeminiCliSetting, GeminiCliCredential);
