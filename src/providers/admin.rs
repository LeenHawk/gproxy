use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::Value;

use crate::context::AppContext;

#[async_trait]
pub trait ProviderAdmin: Send + Sync {
    async fn get_config(&self, ctx: &AppContext) -> Result<Value, StatusCode>;
    async fn put_config(&self, ctx: &AppContext, config: Value) -> Result<Value, StatusCode>;
    async fn list_credentials(&self, ctx: &AppContext) -> Result<Value, StatusCode>;
    async fn add_credential(&self, ctx: &AppContext, credential: Value) -> Result<(), StatusCode>;
    async fn update_credential(
        &self,
        ctx: &AppContext,
        index: usize,
        credential: Value,
    ) -> Result<(), StatusCode>;
    async fn delete_credential(&self, ctx: &AppContext, index: usize) -> Result<(), StatusCode>;
}
