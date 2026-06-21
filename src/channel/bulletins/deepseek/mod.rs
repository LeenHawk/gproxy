//! DeepSeek channel.
//!
//! Two upstream auth surfaces share one host:
//! - OpenAI-compatible `/chat/completions` (+ models) use
//!   `Authorization: Bearer`.
//! - The Anthropic-compatible `/anthropic/v1/messages` endpoint (reached by the
//!   `cg(ClaudeMessages)` passthrough) uses `x-api-key` — [`auth`] rehomes the
//!   inbound `/v1/messages` path and picks the scheme.
//!
//! The OpenAI chat path strips a set of request fields DeepSeek rejects and
//! fixes up a few response fields — see [`shape`].

mod auth;
mod shape;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{allow_headers, allow_query, build_request, join_url};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.deepseek.com"),
    forward_headers: &[],
    forward_query: &[],
};

/// Whether `op` targets DeepSeek's OpenAI `/chat/completions` surface — the only
/// surface whose request/response bodies need shaping.
fn is_openai_chat(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && op.kind == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChatCompletions)
}

pub struct DeepSeekChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for DeepSeekChannel {
    fn id(&self) -> &'static str {
        "deepseek"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
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
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            // === Generate content (stream) ===
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
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
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        // Rehome the inbound Claude-messages path onto DeepSeek's
        // Anthropic-compat surface before building, so auth keys off the real
        // upstream path. `common::build_request` is inlined here because it
        // consumes `ctx.path` verbatim and we need the rewritten path.
        let path = auth::upstream_path(ctx.path).to_string();
        let base_url = common::resolve_base_url(&ctx, &DEFAULTS)?;
        let api_key = common::resolve_api_key(&ctx)?;
        let query = allow_query(ctx.query, DEFAULTS.forward_query);
        let uri = join_url(&base_url, &path, query.as_deref())?;
        let headers = allow_headers(ctx.headers, DEFAULTS.forward_headers);
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
mod tests {
    use super::*;
    use http::Method;
    use serde_json::json;

    fn prepare(path: &str) -> http::Request<Bytes> {
        let secret = json!({ "api_key": "sk-deepseek" });
        let settings = json!({});
        let headers = HeaderMap::new();
        DeepSeekChannel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "deepseek-chat",
                method: Method::POST,
                path,
                query: None,
                headers: &headers,
                body: Bytes::from_static(b"{}"),
            })
            .unwrap()
            .request
    }

    #[test]
    fn claude_messages_path_rehomed_with_x_api_key() {
        let req = prepare("/v1/messages");
        assert_eq!(
            req.uri().to_string(),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
        assert_eq!(req.headers().get("x-api-key").unwrap(), "sk-deepseek");
        assert!(req.headers().get("authorization").is_none());
    }

    #[test]
    fn openai_chat_path_uses_bearer() {
        let req = prepare("/v1/chat/completions");
        assert_eq!(
            req.uri().to_string(),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer sk-deepseek"
        );
        assert!(req.headers().get("x-api-key").is_none());
    }
}
