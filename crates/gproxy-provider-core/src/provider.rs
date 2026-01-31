use async_trait::async_trait;

use std::sync::Arc;

use crate::request::ProxyRequest;
use crate::response::{ProxyResponse, UpstreamPassthroughError};
use crate::traffic::{DownstreamRecordMeta, NoopTrafficSink, SharedTrafficSink};

#[derive(Clone)]
pub struct DownstreamContext {
    pub trace_id: String,
    pub request_id: Option<String>,
    pub user_id: Option<String>,
    pub user_key_id: Option<String>,
    pub proxy: Option<String>,
    pub traffic: SharedTrafficSink,
    pub downstream_meta: Option<DownstreamRecordMeta>,
    pub user_agent: Option<String>,
}

impl Default for DownstreamContext {
    fn default() -> Self {
        Self {
            trace_id: String::new(),
            request_id: None,
            user_id: None,
            user_key_id: None,
            proxy: None,
            traffic: Arc::new(NoopTrafficSink),
            downstream_meta: None,
            user_agent: None,
        }
    }
}

#[derive(Clone)]
pub struct UpstreamContext {
    pub trace_id: String,
    pub provider_id: Option<i64>,
    pub proxy: Option<String>,
    pub traffic: SharedTrafficSink,
    pub user_agent: Option<String>,
}

impl DownstreamContext {
    pub fn upstream(&self) -> UpstreamContext {
        UpstreamContext {
            trace_id: self.trace_id.clone(),
            provider_id: self
                .downstream_meta
                .as_ref()
                .and_then(|meta| meta.provider_id),
            proxy: self.proxy.clone(),
            traffic: self.traffic.clone(),
            user_agent: self.user_agent.clone(),
        }
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;

    async fn call(
        &self,
        req: ProxyRequest,
        ctx: DownstreamContext,
    ) -> Result<ProxyResponse, UpstreamPassthroughError>;
}
