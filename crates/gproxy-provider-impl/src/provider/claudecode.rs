use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use gproxy_provider_core::{
    CallContext, CredentialPool, PoolSnapshot, Provider, ProxyRequest, ProxyResponse, StateSink,
    UpstreamPassthroughError,
};

use crate::credential::BaseCredential;
use crate::ProviderDefault;
use crate::provider::not_implemented;

pub const PROVIDER_NAME: &str = "claudecode";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({ "base_url": DEFAULT_BASE_URL }),
        enabled: true,
    }
}

#[derive(Debug)]
pub struct ClaudeCodeProvider {
    pool: CredentialPool<ClaudeCodeCredential>,
}

pub type ClaudeCodeCredential = BaseCredential;

impl ClaudeCodeProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<ClaudeCodeCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<ClaudeCodeCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for ClaudeCodeProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn call(
        &self,
        _req: ProxyRequest,
        _ctx: CallContext,
    ) -> Result<ProxyResponse, UpstreamPassthroughError> {
        Err(not_implemented(PROVIDER_NAME))
    }
}
