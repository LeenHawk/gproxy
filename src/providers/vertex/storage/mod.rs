// Macro contract: provider_storage_modules! expands to cfg-gated backend modules:
// database/file/memory/s3 based on storage-* features.
crate::provider_storage_modules!();

use anyhow::Result;
use async_trait::async_trait;

use crate::providers::vertex::{VertexCredential, VertexSetting};

pub struct VertexStorage<'a, S> {
    storage: &'a S,
}

#[async_trait]
pub trait VertexBackend: Send + Sync {
    async fn get_config(&self) -> Result<VertexSetting>;
    async fn load_config(&self) -> Result<VertexSetting>;
    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexSetting) + Send;

    async fn get_credentials(&self) -> Result<Vec<VertexCredential>>;
    async fn load_credentials(&self) -> Result<Vec<VertexCredential>>;
    async fn add_credential(&self, credential: VertexCredential) -> Result<()>;
    async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexCredential) + Send;
    async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexCredential) + Send;
    async fn delete_credential(&self, project_id: &str) -> Result<()>;
    async fn get_credential(&self, index: usize) -> Result<Option<VertexCredential>>;
}

impl<'a, S> VertexStorage<'a, S>
where
    S: VertexBackend,
{
    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }

    pub async fn get_config(&self) -> Result<VertexSetting> {
        self.storage.get_config().await
    }

    pub async fn load_config(&self) -> Result<VertexSetting> {
        self.storage.load_config().await
    }

    pub async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexSetting) + Send,
    {
        self.storage.update_config(update).await
    }

    pub async fn get_credentials(&self) -> Result<Vec<VertexCredential>> {
        self.storage.get_credentials().await
    }

    pub async fn load_credentials(&self) -> Result<Vec<VertexCredential>> {
        self.storage.load_credentials().await
    }

    pub async fn add_credential(&self, credential: VertexCredential) -> Result<()> {
        self.storage.add_credential(credential).await
    }

    pub async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexCredential) + Send,
    {
        self.storage.update_credential(index, update).await
    }

    pub async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexCredential) + Send,
    {
        self.storage.update_credential_by_id(id, update).await
    }

    pub async fn delete_credential(&self, project_id: &str) -> Result<()> {
        self.storage.delete_credential(project_id).await
    }

    pub async fn get_credential(&self, index: usize) -> Result<Option<VertexCredential>> {
        self.storage.get_credential(index).await
    }
}

// Macro contract: provider_storage_backend! expands to StorageService dispatch for all backend
// operations plus a default "storage not configured" error arm.
crate::provider_storage_backend!(VertexBackend, VertexSetting, VertexCredential);
