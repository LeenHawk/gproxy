//! Claude Code channel: Anthropic Messages over OAuth2 with the Claude CLI
//! fingerprint. Requests remain verbatim Claude Messages without an envelope or
//! response decoder.

mod auth;
mod axios;
mod cch;
mod cookie;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod request;
mod routing;
mod stainless;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
mod token;
mod usage;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::{Value, json};

use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, PrepareCtx, PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::Provider;

pub struct ClaudeCodeChannel;

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub(crate) fn default_emulation() -> wreq::Emulation {
    fingerprint::default_emulation()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for ClaudeCodeChannel {
    fn id(&self) -> &'static str {
        "claudecode"
    }

    fn provider_family(&self) -> Provider {
        Provider::Claude
    }

    /// Claude-subscription account: the OAuth token is account-wide.
    fn credential_wide_auth(&self) -> bool {
        true
    }

    fn cookie_login_requires_browser(&self) -> bool {
        true
    }

    fn refresh_requires_browser(&self, secret: &Value) -> bool {
        let has_cookie = secret
            .get("cookie")
            .and_then(Value::as_str)
            .is_some_and(|cookie| !cookie.trim().is_empty());
        let has_refresh_token = secret
            .get("refresh_token")
            .and_then(Value::as_str)
            .is_some_and(|token| !token.trim().is_empty());
        has_cookie && !has_refresh_token
    }

    /// All models draw the account 5h/weekly MAIN pool; fable
    /// (`claude-fable-5`) has an ADDITIONAL weekly-scoped limit on top
    /// (`weekly_scoped:fable` in the usage snapshot), so its own 429 stays
    /// model-scoped while a main-pool 429 blocks the whole credential.
    fn shares_account_quota(&self, upstream_model_id: &str) -> bool {
        !upstream_model_id.to_ascii_lowercase().contains("fable")
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routing::table()
    }

    /// Apply Claude request hygiene, including unsupported prefill coercion.
    fn shape_request(&self, body: Bytes, headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        request::shape(body, headers, ctx)
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        request::prepare(ctx)
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
        usage::request(secret, settings)
    }

    fn parse_usage(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        usage::parse(status, body)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for ClaudeCodeChannel {
    async fn authcode_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeStartCtx<'_>,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let (authorize_url, redirect_uri) =
            auth::authcode_start(ctx.redirect_uri, ctx.state, ctx.pkce_challenge);
        Ok(Some(AuthCodeStart {
            authorize_url,
            redirect_uri,
            extra: Some(json!({ "state": ctx.state })),
        }))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeExchangeCtx<'_>,
    ) -> Result<Value, ChannelError> {
        let state = ctx
            .extra
            .and_then(|value| value.get("state"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ChannelError::Build("claudecode login session missing state".into()))?;
        auth::authcode_exchange(client, ctx.code, ctx.verifier, ctx.redirect_uri, state).await
    }

    async fn cookie_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::CookieExchangeCtx<'_>,
    ) -> Result<Value, ChannelError> {
        cookie::exchange(client, ctx.cookie).await
    }
}
