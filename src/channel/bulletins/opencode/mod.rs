//! OpenCode Zen / Go channels — one gateway host, two tiers, four wire surfaces.
//!
//! Zen (`https://opencode.ai/zen/v1`) is pay-as-you-go over OpenCode's full
//! curated catalogue; Go (`https://opencode.ai/zen/go/v1`) is the flat
//! subscription over the open-weights subset. Both are the same gateway, and it
//! converts between wire formats server-side — so ANY surface serves ANY model
//! in that tier and every declared content cell is a plain passthrough. The
//! upstream path and the credential header are chosen from the routed cell (the
//! internal `auth` module); the tier only changes the base URL.
//!
//! The gateway credential is an API key. It can be pasted directly, or supplied
//! by the OpenCode Console device-code exchange; the latter is refreshed as an
//! OAuth credential while still feeding the same API-key request path.
//! There is no envelope, no stream decoder, and no TLS impersonation, which is
//! why both tiers are in the edge/wasm channel subset. Live per-credential usage
//! is not exposed by any public OpenCode endpoint, so neither tier reports a
//! usage snapshot.

mod auth;
mod console;
mod login;
mod routes;

use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use serde_json::Value;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{allow_headers, allow_query, build_request};
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{self, claude_cache_control, claude_magic_cache, openai_cache};
use crate::channel::{
    Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx, PreparedRequest,
    RefreshCtx, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{ContentGenerationKind, OperationKind};

/// Zen: pay-as-you-go, full catalogue, Gemini served natively.
#[cfg(feature = "channel-opencodezen")]
pub struct OpenCodeZenChannel;

/// Go: flat subscription, open-weights subset, no Gemini surface.
#[cfg(feature = "channel-opencodego")]
pub struct OpenCodeGoChannel;

#[cfg(feature = "channel-opencodezen")]
const ZEN_DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://opencode.ai/zen/v1"),
    forward_headers: &[],
    forward_query: FORWARD_QUERY,
};

#[cfg(feature = "channel-opencodego")]
const GO_DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://opencode.ai/zen/go/v1"),
    forward_headers: &[],
    forward_query: FORWARD_QUERY,
};

/// `alt=sse` rides the Gemini surface. The gateway itself keys streaming off the
/// `:streamGenerateContent` verb, but it forwards the query to the Google
/// upstream it selects, which does need it.
const FORWARD_QUERY: &[&str] = &["alt"];

/// Build the upstream request for one tier. `common::build_request` is inlined
/// because it consumes `ctx.path` verbatim, and this channel derives the path
/// from the routed cell instead.
fn prepare(ctx: PrepareCtx<'_>, d: &ApiKeyDefaults) -> Result<PreparedRequest, ChannelError> {
    let op = ctx.op;
    let path = auth::upstream_path(op, ctx.stream, ctx.upstream_model_id)?;
    let api_key = common::resolve_api_key(&ctx)?;
    let query = allow_query(ctx.query, d.forward_query);
    let uri = common::resolve_uri(&ctx, d, &path, query.as_deref())?;
    // The gateway supplies its own credential header; nothing inbound forwards.
    let headers = allow_headers(ctx.headers, d.forward_headers);
    let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
    auth::apply(&mut req, op, &api_key)?;
    Ok(PreparedRequest::new(req))
}

/// Opt-in magic-string cache breakpoints, per inbound protocol. Both switches
/// default off; the Claude surface additionally gets `cache_control` hygiene so
/// a hand-written body cannot exceed Anthropic's breakpoint limit.
fn shape(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    let settings = RequestShapeSettings::from_value(ctx.settings);
    if let Some(kind) = openai_cache::kind_for_operation(ctx.op) {
        if !settings.enable_openai_magic_cache {
            return body;
        }
        return shaping::with_json_body(body, |value| {
            openai_cache::apply_magic_string_cache_breakpoints(value, kind)
        });
    }
    if ctx.op.kind() != OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        || !settings.enable_claude_magic_cache
    {
        return body;
    }
    shaping::with_json_body(body, |value| {
        claude_magic_cache::apply_magic_string_cache_control_triggers(value);
        claude_cache_control::sanitize_claude_body(value);
    })
}

#[cfg(feature = "channel-opencodezen")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for OpenCodeZenChannel {
    fn id(&self) -> &'static str {
        "opencodezen"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routes::zen()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        prepare(ctx, &ZEN_DEFAULTS)
    }

    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        shape(body, ctx)
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        login::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: RefreshCtx<'_>,
    ) -> Result<Value, ChannelError> {
        login::refresh(client, ctx.secret, ctx.provider_settings).await
    }
}

/// Device-code login against the OpenCode Console. The minted access token is
/// stored as the gateway key together with the fields needed to refresh it.
#[cfg(feature = "channel-opencodezen")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for OpenCodeZenChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DeviceStartCtx<'_>,
    ) -> Result<DeviceInit, ChannelError> {
        login::device_start(client, ctx.provider_settings).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DevicePollCtx<'_>,
    ) -> Result<DevicePoll, ChannelError> {
        login::device_poll(client, ctx.provider_settings, ctx.device_code).await
    }
}

#[cfg(feature = "channel-opencodego")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for OpenCodeGoChannel {
    fn id(&self) -> &'static str {
        "opencodego"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routes::go()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        prepare(ctx, &GO_DEFAULTS)
    }

    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        shape(body, ctx)
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        login::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: RefreshCtx<'_>,
    ) -> Result<Value, ChannelError> {
        login::refresh(client, ctx.secret, ctx.provider_settings).await
    }
}

/// Same Console login as Zen; the returned access credential is valid for the
/// account's subscribed tier and the channel chooses the Go gateway URL.
#[cfg(feature = "channel-opencodego")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for OpenCodeGoChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DeviceStartCtx<'_>,
    ) -> Result<DeviceInit, ChannelError> {
        login::device_start(client, ctx.provider_settings).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DevicePollCtx<'_>,
    ) -> Result<DevicePoll, ChannelError> {
        login::device_poll(client, ctx.provider_settings, ctx.device_code).await
    }
}

// Both tiers are exercised together — the pair's only real divergence is the
// Gemini cell, which needs both routing tables in one assertion.
#[cfg(all(
    test,
    not(target_arch = "wasm32"),
    feature = "channel-opencodezen",
    feature = "channel-opencodego"
))]
mod tests;
