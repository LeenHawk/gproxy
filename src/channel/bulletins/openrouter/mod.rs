//! OpenRouter channel — `Authorization: Bearer`, default `https://openrouter.ai/api`.

mod auth;
mod shape;

use bytes::Bytes;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::shaping::{
    self, claude_cache_control, claude_fallback, claude_magic_cache, openai_cache,
};
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
        ];
        routes.extend(responses_ws_to(cg(OpenAiResponses)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }

    /// Opt-in magic-string cache triggers for native Claude and OpenAI bodies.
    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if let Some(kind) = openai_cache::kind_for_operation(ctx.op) {
            if !ctx.enable_magic_cache {
                return body;
            }
            return shaping::with_json_body(body, |value| {
                openai_cache::apply_magic_string_cache_breakpoints(value, kind)
            });
        }
        if !is_claude_messages(ctx.op)
            || (!ctx.enable_magic_cache && !ctx.enable_claude_fable_fallback)
        {
            return body;
        }
        shaping::with_json_body(body, |v| {
            if ctx.enable_magic_cache {
                claude_magic_cache::apply_magic_string_cache_control_triggers(v);
                claude_cache_control::sanitize_claude_body(v);
            }
            if ctx.enable_claude_fable_fallback && v.get("models").is_none() {
                claude_fallback::apply_fable_to_opus48_body_only(v);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderMap, StatusCode};
    use serde_json::{Value, json};

    use crate::protocol::{Operation, OperationKey};

    fn fallback_ctx() -> ShapeCtx {
        ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            status: StatusCode::OK,
            enable_magic_cache: false,
            enable_claude_fable_fallback: true,
        }
    }

    fn openai_magic_ctx() -> ShapeCtx {
        ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            stream: false,
            status: StatusCode::OK,
            enable_magic_cache: true,
            enable_claude_fable_fallback: false,
        }
    }

    #[test]
    fn shapes_openai_magic_cache_breakpoint() {
        let mut headers = HeaderMap::new();
        let body = Bytes::from_static(
            br#"{"model":"openai/gpt-5.6","input":[{"role":"user","content":[{"type":"input_text","text":"stable GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH"}]}]}"#,
        );
        let shaped = OpenRouterChannel.shape_request(body, &mut headers, &openai_magic_ctx());
        let value: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(
            value["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
    }

    #[test]
    fn injects_openrouter_fable_fallback_without_anthropic_beta() {
        let mut headers = HeaderMap::new();
        let body =
            Bytes::from(r#"{"model":"anthropic/claude-fable-5","messages":[],"max_tokens":32}"#);
        let shaped = OpenRouterChannel.shape_request(body, &mut headers, &fallback_ctx());

        let v: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(
            v["fallbacks"],
            json!([{ "model": "anthropic/claude-opus-4-8" }])
        );

        let secret = json!({ "api_key": "or-test" });
        let settings = json!({});
        let req = OpenRouterChannel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "anthropic/claude-fable-5",
                method: http::Method::POST,
                path: "/v1/messages",
                query: None,
                headers: &headers,
                body: shaped,
            })
            .unwrap()
            .into_http();

        assert!(req.headers().get("anthropic-beta").is_none());
    }

    #[test]
    fn does_not_combine_fallbacks_with_openrouter_models() {
        let mut headers = HeaderMap::new();
        let body = Bytes::from(
            r#"{"model":"anthropic/claude-fable-5","models":["anthropic/claude-fable-5","anthropic/claude-opus-4-8"],"messages":[],"max_tokens":32}"#,
        );
        let shaped = OpenRouterChannel.shape_request(body, &mut headers, &fallback_ctx());

        let v: Value = serde_json::from_slice(&shaped).unwrap();
        assert!(v.get("fallbacks").is_none());
        assert!(headers.get("anthropic-beta").is_none());
    }
}
