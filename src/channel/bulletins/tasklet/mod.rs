//! Tasklet consumer-web channel over its asynchronous Agent API.

mod auth;
mod bridge;
mod login;
pub(crate) mod mcp;
mod models;
mod registration;
mod registration_support;
mod request;
mod response;
mod routing;
mod session;
mod stream;
#[cfg(test)]
mod tests;

use bytes::Bytes;

use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, PrepareCtx, PreparedRequest,
};
use crate::http::client::UpstreamClient;
use crate::protocol::Provider;

pub struct TaskletChannel;

impl TaskletChannel {
    pub const ID: &'static str = "tasklet";
}

#[async_trait::async_trait]
impl Channel for TaskletChannel {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routing::table()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        session::prepare(ctx)
    }

    fn bundled_models(&self) -> Option<Bytes> {
        Some(models::catalog())
    }
}

#[async_trait::async_trait]
impl ChannelLogin for TaskletChannel {
    async fn authcode_start(
        &self,
        client: &std::sync::Arc<dyn UpstreamClient>,
        params: &serde_json::Value,
        _redirect_uri: &str,
        _state: &str,
        _pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        login::start(client, params).await.map(Some)
    }

    async fn authcode_exchange(
        &self,
        client: &std::sync::Arc<dyn UpstreamClient>,
        pin: &str,
        _verifier: &str,
        _redirect_uri: &str,
        extra: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, ChannelError> {
        login::exchange(client, pin, extra).await
    }
}
