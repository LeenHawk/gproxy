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

use crate::channel::{Channel, ChannelError, ChannelLogin, PrepareCtx, PreparedRequest};
use crate::http::client::UpstreamClient;
use crate::protocol::Provider;

pub struct ClaudeWebChannel;

impl ClaudeWebChannel {
    pub const ID: &'static str = "claudeweb";
}

#[async_trait::async_trait]
impl Channel for ClaudeWebChannel {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn provider_family(&self) -> Provider {
        Provider::Claude
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routing::table()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        session::prepare(ctx)
    }

    fn credential_models(&self, secret: &Value) -> Option<Bytes> {
        models::credential_models(secret)
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, secret).await
    }

    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
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
        cookie: &str,
    ) -> Result<Value, ChannelError> {
        auth::exchange(client, cookie).await
    }
}
