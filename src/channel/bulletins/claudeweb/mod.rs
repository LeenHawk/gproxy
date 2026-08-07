//! Claude consumer-web channel — Anthropic Messages over a claude.ai browser
//! session cookie.
//!
//! A web turn is inherently multi-step: upload attachments, create a temporary
//! conversation, set its thinking mode, then stream `/completion`. The channel
//! therefore uses `PreparedRequest::CustomStream` and is native-only. Request
//! construction follows Clewdr/Clove; the stream decoder accepts both their
//! legacy `{completion}` and modern Messages-SSE response shapes.

mod auth;
mod fingerprint;
mod models;
mod request;
mod response;
mod routing;
mod session;
mod state;
#[cfg(test)]
mod tests;
mod usage;

use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use serde_json::Value;

use crate::channel::{
    Channel, ChannelError, ChannelLogin, PrepareCtx, PreparedRequest,
};
use crate::http::client::UpstreamClient;
use crate::protocol::Provider;

pub struct ClaudeWebChannel;

impl ClaudeWebChannel {
    pub const ID: &'static str = "claudeweb";
}

pub(crate) fn default_emulation() -> wreq::Emulation {
    fingerprint::default_emulation()
}

#[async_trait::async_trait]
impl Channel for ClaudeWebChannel {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routing::table()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        session::prepare(ctx)
    }

    fn credential_models(&self, secret: &Value) -> Option<ModelCatalog> {
        models::credential_models(secret).map(|body| ModelCatalog {
            family: Provider::Claude,
            body,
        })
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::RefreshCtx<'_>,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, ctx.secret).await
    }

    fn prepare_usage_request(
        &self,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<Request<Bytes>>, ChannelError> {
        usage::request(secret, settings)
    }

    fn parse_usage(
        &self,
        status: StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        usage::parse(status, body)
    }
}

#[async_trait::async_trait]
impl ChannelLogin for ClaudeWebChannel {
    async fn cookie_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::CookieExchangeCtx<'_>,
    ) -> Result<Value, ChannelError> {
        auth::exchange(client, ctx.cookie).await
    }
}
