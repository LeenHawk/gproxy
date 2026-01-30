use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use gproxy_provider_core::{
    CredentialPool, DownstreamContext, PoolSnapshot, Provider, ProxyRequest, ProxyResponse,
    StateSink, UpstreamPassthroughError,
};

use crate::credential::BaseCredential;
use crate::ProviderDefault;
use crate::provider::not_implemented;

pub const PROVIDER_NAME: &str = "geminicli";
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({ "base_url": DEFAULT_BASE_URL }),
        enabled: true,
    }
}

#[derive(Debug)]
pub struct GeminiCliProvider {
    pool: CredentialPool<GeminiCliCredential>,
}

pub type GeminiCliCredential = BaseCredential;

impl GeminiCliProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<GeminiCliCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<GeminiCliCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for GeminiCliProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn call(
        &self,
        _req: ProxyRequest,
        _ctx: DownstreamContext,
    ) -> Result<ProxyResponse, UpstreamPassthroughError> {
        Err(not_implemented(PROVIDER_NAME))
    }
}
