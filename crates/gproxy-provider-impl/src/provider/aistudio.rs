use std::sync::Arc;

use async_trait::async_trait;

use gproxy_provider_core::{
    CallContext, CredentialPool, PoolSnapshot, Provider, ProxyRequest, ProxyResponse, StateSink,
    UpstreamPassthroughError,
};

use crate::credential::BaseCredential;
use crate::provider::not_implemented;

pub const PROVIDER_NAME: &str = "aistudio";

#[derive(Debug)]
pub struct AistudioProvider {
    pool: CredentialPool<AistudioCredential>,
}

pub type AistudioCredential = BaseCredential;

impl AistudioProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<AistudioCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<AistudioCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for AistudioProvider {
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
