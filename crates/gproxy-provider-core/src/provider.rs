use async_trait::async_trait;

use crate::request::ProxyRequest;
use crate::response::{ProxyResponse, UpstreamPassthroughError};

#[derive(Debug, Clone, Default)]
pub struct CallContext {
    pub request_id: Option<String>,
    pub user_id: Option<String>,
    pub user_key_id: Option<String>,
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
