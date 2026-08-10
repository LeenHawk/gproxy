//! Gemini CLI channel: Google Code Assist OAuth2 and request/response envelope.

mod auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod models;
mod request;
mod response;
mod routing;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::envelope;
use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, ChannelStreamDecoder, PrepareCtx,
    PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;

pub struct GeminiCliChannel;

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub(crate) fn default_emulation() -> wreq::Emulation {
    fingerprint::default_emulation()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for GeminiCliChannel {
    fn id(&self) -> &'static str {
        "geminicli"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routing::table()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        request::prepare(ctx)
    }

    fn shape_request(&self, body: Bytes, headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        request::shape(body, headers, ctx)
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        response::shape(body, ctx)
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(response::stream_decoder())
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
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        let access_token = auth::access_token(secret)?;
        let project_id = auth::project_id(secret)?;
        let user_agent = auth::user_agent("gemini-2.5-pro");
        match crate::channel::settings::endpoint_by_key(settings, "usage", "") {
            Some(url) => {
                envelope::user_quota_request_at(&url, access_token, project_id, &user_agent)
            }
            None => {
                let base = settings
                    .get("base_url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|base| !base.is_empty())
                    .unwrap_or(auth::BASE_URL);
                envelope::user_quota_request(base, access_token, project_id, &user_agent)
            }
        }
    }

    fn parse_usage(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        envelope::parse_user_quota(status, body)
    }

    fn describe_usage_window(
        &self,
        snapshot: &crate::channel::UsageSnapshot,
        index: usize,
    ) -> crate::channel::UsageWindowDescriptor {
        envelope::describe_user_quota_window(snapshot, index)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for GeminiCliChannel {
    async fn authcode_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeStartCtx<'_>,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let effective = if !ctx.redirect_uri.trim().is_empty() {
            ctx.redirect_uri
        } else if ctx.params.get("code_only").and_then(Value::as_bool) == Some(false) {
            auth::LOOPBACK_REDIRECT_URI
        } else {
            auth::DEFAULT_REDIRECT_URI
        };
        let (authorize_url, redirect_uri) =
            auth::authcode_start(effective, ctx.state, ctx.pkce_challenge);
        Ok(Some(AuthCodeStart {
            authorize_url,
            redirect_uri,
            extra: None,
        }))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeExchangeCtx<'_>,
    ) -> Result<Value, ChannelError> {
        auth::authcode_exchange(client, ctx.code, ctx.verifier, ctx.redirect_uri).await
    }
}
