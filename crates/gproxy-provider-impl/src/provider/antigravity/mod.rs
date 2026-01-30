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

pub const PROVIDER_NAME: &str = "antigravity";

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({}),
        enabled: true,
    }
}

#[derive(Debug)]
pub struct AntiGravityProvider {
    pool: CredentialPool<AntiGravityCredential>,
}

pub type AntiGravityCredential = BaseCredential;

impl AntiGravityProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<AntiGravityCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<AntiGravityCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for AntiGravityProvider {
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
