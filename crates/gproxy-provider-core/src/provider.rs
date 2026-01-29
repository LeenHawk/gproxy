use async_trait::async_trait;

use std::sync::Arc;

use crate::request::ProxyRequest;
use crate::response::{ProxyResponse, UpstreamPassthroughError};
use crate::traffic::{DownstreamRecordMeta, NoopTrafficSink, SharedTrafficSink};

#[derive(Clone)]
pub struct CallContext {
    pub trace_id: String,
    pub request_id: Option<String>,
    pub user_id: Option<String>,
    pub user_key_id: Option<String>,
    pub proxy: Option<String>,
    pub traffic: SharedTrafficSink,
    pub downstream_meta: Option<DownstreamRecordMeta>,
}

impl Default for CallContext {
    fn default() -> Self {
        Self {
            trace_id: String::new(),
            request_id: None,
            user_id: None,
            user_key_id: None,
            proxy: None,
            traffic: Arc::new(NoopTrafficSink),
            downstream_meta: None,
        }
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;

    async fn call(
        &self,
        req: ProxyRequest,
        ctx: CallContext,
    ) -> Result<ProxyResponse, UpstreamPassthroughError>;
}
