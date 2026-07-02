//! Anthropic Claude channel — `x-api-key` + `anthropic-version`, default
//! `https://api.anthropic.com`.

mod auth;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::shaping::{
    self, claude_cache_control, claude_fallback, claude_magic_cache, claude_sampling,
};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, OperationKind, Provider};

/// Whether `op` targets the Claude-messages content-generation path (the only
/// route that carries a Claude request body to shape).
fn is_claude_messages(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    )
}

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.anthropic.com"),
    forward_headers: &["anthropic-beta"],
    forward_query: &[],
};

pub struct ClaudeApiChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for ClaudeApiChannel {
    fn id(&self) -> &'static str {
        "claudeapi"
    }

    fn provider_family(&self) -> Provider {
        Provider::Claude
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::Claude)),
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::Claude)),
            pass(GetModel, pv(P::Claude)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::Claude)),
            pass(CountTokens, pv(P::Claude)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Claude)),
            xform(CountTokens, pv(P::Gemini), CountTokens, pv(P::Claude)),
            pass(GenerateContent, cg(ClaudeMessages)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(ClaudeMessages),
            ),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }

    /// Claude request 整形: on the claude-messages content path, sanitize the
    /// body (cache_control hygiene) + strip sampling params, and drop the
    /// `context-1m` beta token that upstream rejects.
    fn shape_request(&self, body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if !is_claude_messages(ctx.op) {
            return body;
        }
        let body = shaping::with_json_body(body, |v| {
            if ctx.enable_magic_cache {
                claude_magic_cache::apply_magic_string_cache_control_triggers(v);
            }
            claude_cache_control::sanitize_claude_body(v);
            claude_sampling::strip_sampling_params(v);
            if ctx.enable_claude_fable_fallback {
                claude_fallback::apply_fable_to_opus48(v, headers);
            }
        });
        shaping::anthropic_beta::strip_beta_tokens(headers, &["context-1m-2025-08-07"]);
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderValue, StatusCode};
    use serde_json::Value;

    use crate::protocol::{Operation, OperationKey};

    fn messages_ctx() -> ShapeCtx {
        ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            status: StatusCode::OK,
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
        }
    }

    fn fallback_ctx() -> ShapeCtx {
        ShapeCtx {
            enable_claude_fable_fallback: true,
            ..messages_ctx()
        }
    }

    #[test]
    fn strips_sampling_and_context_1m_beta() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("context-1m-2025-08-07,files-api-2025-04-14"),
        );
        let body = Bytes::from(
            r#"{"model":"claude-opus-4-8","messages":[],"temperature":0.7,"top_p":0.9,"top_k":40}"#,
        );
        let out = ClaudeApiChannel.shape_request(body, &mut headers, &messages_ctx());

        let v: Value = serde_json::from_slice(&out).unwrap();
        let map = v.as_object().unwrap();
        assert!(!map.contains_key("temperature"));
        assert!(!map.contains_key("top_p"));
        assert!(!map.contains_key("top_k"));
        assert_eq!(
            headers.get("anthropic-beta").unwrap(),
            "files-api-2025-04-14"
        );
    }

    #[test]
    fn non_claude_messages_op_is_identity() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("context-1m-2025-08-07"),
        );
        let body = Bytes::from(r#"{"model":"gpt","temperature":0.7}"#);
        let ctx = ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiChatCompletions,
            ),
            stream: false,
            status: StatusCode::OK,
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
        };
        let out = ClaudeApiChannel.shape_request(body.clone(), &mut headers, &ctx);
        assert_eq!(out, body);
        // header left untouched off-path
        assert_eq!(
            headers.get("anthropic-beta").unwrap(),
            "context-1m-2025-08-07"
        );
    }

    #[test]
    fn injects_fable_server_side_fallback() {
        let mut headers = HeaderMap::new();
        let body = Bytes::from(
            r#"{"model":"claude-fable-5","messages":[],"max_tokens":32,"temperature":0.7}"#,
        );
        let out = ClaudeApiChannel.shape_request(body, &mut headers, &fallback_ctx());

        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["fallbacks"],
            serde_json::json!([{ "model": "claude-opus-4-8" }])
        );
        assert_eq!(
            headers.get("anthropic-beta").unwrap(),
            "server-side-fallback-2026-06-01"
        );
    }
}
