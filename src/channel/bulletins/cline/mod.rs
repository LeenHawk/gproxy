//! Cline channel — the Cline account's OpenAI-compatible inference gateway.
//!
//! One surface, `POST {base}/chat/completions`, serving OpenRouter-style
//! namespaced model ids (`anthropic/claude-sonnet-4.6`). It covers both of
//! Cline's billing products: usage-billed credits and the ClinePass
//! subscription share a base URL and a credential, differing only in which
//! models the account may call, so one channel serves both. Buffered inference
//! replies use Cline's `{success,data}` envelope; streaming replies are already
//! canonical OpenAI Chat SSE.
//!
//! The credential is a Cline account token (device login, refreshable) or a
//! pasted workspace API key; the internal `auth` module knows which prefix each
//! needs. No envelope, no stream decoder, no TLS impersonation, so the channel
//! is in the edge/wasm subset.

mod auth;
mod login;
mod model_list;
mod response;
mod usage;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{
    allow_headers_with_settings, allow_query_with_settings, build_request,
};
use crate::channel::{
    Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx, PreparedRequest,
    RefreshCtx, UsageSnapshot,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{Operation, OperationKey};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.cline.bot/api/v1"),
    forward_headers: &[],
    forward_query: &[],
};

/// Base URL for the account API and the inference surface — one host serves
/// both. Self-hosted and staging deployments override it.
fn base_url(settings: &Value) -> &str {
    settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(DEFAULTS.default_base_url.expect("baked default"))
        .trim_end_matches('/')
}

pub struct ClineChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for ClineChannel {
    fn id(&self) -> &'static str {
        "cline"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            // Cline exposes a catalogue and a chat surface, nothing per-model.
            local(GetModel, pv(P::OpenAi)),
            local(GetModel, pv(P::Claude)),
            local(GetModel, pv(P::Gemini)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
        ];
        routes.extend(responses_ws_to(cg(OpenAiChatCompletions)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        // Keyed off the routed cell, not the inbound path: a transformed
        // candidate still carries the downstream client's original path.
        let path = upstream_path(ctx.op);
        let query =
            allow_query_with_settings(ctx.query, DEFAULTS.forward_query, ctx.provider_settings);
        let uri = common::resolve_uri(&ctx, &DEFAULTS, path, query.as_deref())?;
        let headers = allow_headers_with_settings(
            ctx.headers,
            DEFAULTS.forward_headers,
            ctx.provider_settings,
        );
        let secret = ctx.secret;
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, secret)?;
        Ok(PreparedRequest::new(req))
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
    ) -> Option<UsageSnapshot> {
        usage::parse(status, body)
    }

    fn shape_response(&self, body: Bytes, ctx: &crate::channel::ShapeCtx) -> Bytes {
        if ctx.op.operation() == Operation::ListModels {
            model_list::to_openai(body)
        } else {
            response::unwrap_chat(body)
        }
    }
}

/// Only the catalogue is not the chat surface.
fn upstream_path(op: OperationKey) -> &'static str {
    if op.operation() == Operation::ListModels {
        "/ai/cline/recommended-models"
    } else {
        "/chat/completions"
    }
}

/// Cline delegates identity to WorkOS, so the device flow runs there and the
/// resulting pair is registered with Cline for the tokens that authorize
/// inference. No authcode flow: it needs a localhost callback listener.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for ClineChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        _ctx: crate::channel::DeviceStartCtx<'_>,
    ) -> Result<DeviceInit, ChannelError> {
        login::device_start(client).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DevicePollCtx<'_>,
    ) -> Result<DevicePoll, ChannelError> {
        login::device_poll(client, ctx.provider_settings, ctx.device_code).await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
