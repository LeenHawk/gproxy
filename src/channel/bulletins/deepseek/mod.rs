//! DeepSeek channel.
//!
//! Three upstream API surfaces share one host:
//! - OpenAI-compatible `/chat/completions`, `/responses` (+ models) use
//!   `Authorization: Bearer`.
//! - The Anthropic-compatible `/anthropic/v1/messages` endpoint (reached by the
//!   `cg(ClaudeMessages)` passthrough) uses `x-api-key` — the internal `auth` module rehomes the
//!   inbound `/v1/messages` path and picks the scheme.
//!
//! The OpenAI chat path strips a set of request fields DeepSeek rejects and
//! fixes up a few response fields — see the internal `shape` module.

mod auth;
mod shape;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{
    allow_headers_with_settings, allow_query_with_settings, build_request,
};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.deepseek.com"),
    forward_headers: &[],
    forward_query: &[],
};

/// Whether `op` targets DeepSeek's OpenAI `/chat/completions` surface — the only
/// surface whose request/response bodies need shaping.
fn is_openai_chat(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.operation(),
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && op.kind() == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChatCompletions)
}

fn is_openai_model_list(op: crate::protocol::OperationKey) -> bool {
    op.operation() == Operation::ListModels
        && op.kind() == OperationKind::Provider(crate::protocol::Provider::OpenAi)
}

pub struct DeepSeekChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for DeepSeekChannel {
    fn id(&self) -> &'static str {
        "deepseek"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            // === Model list/get ===
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            // === Count tokens (local) ===
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            // === Generate content (non-stream) ===
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            // === Generate content (stream) ===
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            // === Compact -> generate ===
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
        ];
        // DeepSeek documents HTTP/SSE Responses, not Responses WebSocket.
        routes.extend(responses_ws_to(cg(OpenAiResponses)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        // Rehome the inbound Claude-messages path onto DeepSeek's
        // Anthropic-compat surface before building, so auth keys off the real
        // upstream path. `common::build_request` is inlined here because it
        // consumes `ctx.path` verbatim and we need the rewritten path.
        let path = auth::upstream_path(ctx.path).to_string();
        let api_key = common::resolve_api_key(&ctx)?;
        let query =
            allow_query_with_settings(ctx.query, DEFAULTS.forward_query, ctx.provider_settings);
        let uri = common::resolve_uri(&ctx, &DEFAULTS, &path, query.as_deref())?;
        let headers = allow_headers_with_settings(
            ctx.headers,
            DEFAULTS.forward_headers,
            ctx.provider_settings,
        );
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &path, &api_key)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if is_openai_chat(ctx.op) {
            shape::shape_request(body)
        } else {
            body
        }
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if ctx.status.is_success() && is_openai_model_list(ctx.op) {
            return shape::shape_model_list(body);
        }
        // Only success bodies on the OpenAI chat surface carry the fields we
        // rewrite; error/non-chat bodies pass through untouched.
        if ctx.status.is_success() && is_openai_chat(ctx.op) {
            shape::shape_response(body)
        } else {
            body
        }
    }
}

#[cfg(test)]
mod tests;
