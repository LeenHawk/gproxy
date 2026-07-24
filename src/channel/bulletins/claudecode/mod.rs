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

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

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
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, secret).await
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
        _params: &Value,
        redirect_uri: &str,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let (authorize_url, redirect_uri) =
            auth::authcode_start(redirect_uri, state, pkce_challenge);
        Ok(Some(AuthCodeStart {
            authorize_url,
            redirect_uri,
            extra: Some(json!({ "state": state })),
        }))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        extra: Option<&Value>,
    ) -> Result<Value, ChannelError> {
        let state = extra
            .and_then(|value| value.get("state"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ChannelError::Build("claudecode login session missing state".into()))?;
        auth::authcode_exchange(client, code, verifier, redirect_uri, state).await
    }

    async fn cookie_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        cookie: &str,
    ) -> Result<Value, ChannelError> {
        cookie::exchange(client, cookie).await
    }
}
