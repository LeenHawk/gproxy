//! OpenRouter channel — `Authorization: Bearer`, default `https://openrouter.ai/api`.

mod auth;
mod shape;

use bytes::Bytes;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::shaping::{self, claude_cache_control, claude_magic_cache};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://openrouter.ai/api"),
    forward_headers: &["http-referer", "x-title"],
    forward_query: &[],
};

pub struct OpenRouterChannel;

/// Whether `op` targets the Claude-messages content path — the only passthrough
/// route that carries a Claude-format body to shape.
fn is_claude_messages(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    )
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for OpenRouterChannel {
    fn id(&self) -> &'static str {
        "openrouter"
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
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiResponses),
            ),
            // === Generate content (stream) ===
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            // === Embeddings ===
            pass(CreateEmbedding, pv(P::OpenAi)),
            xform(
                CreateEmbedding,
                pv(P::Gemini),
                CreateEmbedding,
                pv(P::OpenAi),
            ),
            // === Compact -> generate ===
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(OpenAiResponses),
            ),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }

    /// Opt-in magic-string cache triggers on a Claude-format passthrough body
    /// (provider `enable_magic_cache`). No-op when disabled or non-Claude.
    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if !ctx.enable_magic_cache || !is_claude_messages(ctx.op) {
            return body;
        }
        shaping::with_json_body(body, |v| {
            claude_magic_cache::apply_magic_string_cache_control_triggers(v);
            claude_cache_control::sanitize_claude_body(v);
        })
    }

    /// On `ListModels`, fill the OpenAI model-list shape OpenRouter omits
    /// (top-level `object: "list"`, per-item `object: "model"` + `owned_by`) so
    /// proxy `/v1/models` deserializes strictly. On all other ops, coerce
    /// OpenRouter's int `error.code` to a string and synthesize an OpenAI-style
    /// `error.type` so downstream transforms deserialize error bodies cleanly.
    /// No-op for non-error / already-shaped bodies.
    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if ctx.op.operation == Operation::ListModels {
            shape::reshape_model_list(body)
        } else {
            shape::reshape_error(body)
        }
    }
}
