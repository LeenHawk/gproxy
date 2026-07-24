//! Custom (universal) channel — a generic passthrough to any OpenAI / Claude /
//! Gemini-compatible endpoint. `base_url` or a matching exact endpoint is
//! required because this channel has no baked default; the
//! auth header is chosen by the inbound protocol (see [`auth`]).

mod auth;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{self, claude_cache_control, claude_magic_cache, openai_cache};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, OperationKind, Provider};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: None,
    forward_headers: &[],
    forward_query: &[],
};

pub struct CustomChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for CustomChannel {
    fn id(&self) -> &'static str {
        "custom"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        // Universal transparent passthrough: every (operation, kind) cell the v1
        // custom channel served, mapped to v2 cells. v1 emitted all protocols ×
        // all ops as passthrough; the OpenAI-family protocols collapse to a
        // single provider/content cell each. WebSocket/Live, the *Stream* image
        // ops, the bare-`OpenAi` content cell, and GeminiNDJson have no v2
        // representation and are dropped.
        use crate::channel::routes::{cg, pass, pv};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            pass(ListModels, pv(P::Claude)),
            pass(ListModels, pv(P::Gemini)),
            pass(GetModel, pv(P::OpenAi)),
            pass(GetModel, pv(P::Claude)),
            pass(GetModel, pv(P::Gemini)),
            pass(CountTokens, pv(P::OpenAi)),
            pass(CountTokens, pv(P::Claude)),
            pass(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            pass(GenerateContent, cg(GeminiGenerateContent)),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            pass(StreamGenerateContent, cg(GeminiGenerateContent)),
            pass(CreateEmbedding, pv(P::OpenAi)),
            pass(CreateEmbedding, pv(P::Claude)),
            pass(CreateEmbedding, pv(P::Gemini)),
            pass(CreateImage, pv(P::OpenAi)),
            pass(CreateImage, pv(P::Claude)),
            pass(CreateImage, pv(P::Gemini)),
            pass(EditImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::Claude)),
            pass(EditImage, pv(P::Gemini)),
            pass(CompactContent, pv(P::OpenAi)),
            pass(CompactContent, pv(P::Claude)),
            pass(CompactContent, pv(P::Gemini)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        // Decide the auth style from the inbound path BEFORE `ctx` is consumed.
        let proto = auth::detect(ctx.path);
        let anthropic_beta = (ctx.method == http::Method::POST && ctx.path == "/v1/messages")
            .then(|| ctx.headers.get("anthropic-beta").cloned())
            .flatten();
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        if let Some(value) = anthropic_beta {
            req.headers_mut().insert("anthropic-beta", value);
        }
        auth::apply(&mut req, &key, proto)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        let settings = RequestShapeSettings::from_value(ctx.settings);
        if let Some(kind) = openai_cache::kind_for_operation(ctx.op) {
            if !settings.enable_openai_magic_cache {
                return body;
            }
            return shaping::with_json_body(body, |value| {
                openai_cache::apply_magic_string_cache_breakpoints(value, kind)
            });
        }
        if ctx.op.kind != OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
            || (!settings.enable_claude_magic_cache && settings.claude_fable_fallbacks.is_none())
        {
            return body;
        }
        shaping::with_json_body(body, |value| {
            if settings.enable_claude_magic_cache {
                claude_magic_cache::apply_magic_string_cache_control_triggers(value);
                claude_cache_control::sanitize_claude_body(value);
            }
            if let Some(fallbacks) = settings.claude_fable_fallbacks.as_ref() {
                shaping::claude_fallback::apply_fable_fallback(value, headers, fallbacks);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use serde_json::{Value, json};

    use crate::protocol::{Operation, OperationKey};

    const MAGIC: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

    fn ctx(kind: ContentGenerationKind, settings: &Value) -> ShapeCtx<'_> {
        ShapeCtx {
            op: OperationKey::content_generation(Operation::GenerateContent, kind),
            stream: false,
            status: StatusCode::OK,
            settings,
        }
    }

    #[test]
    fn magic_cache_switches_are_protocol_specific() {
        let openai_settings = json!({ "enable_openai_magic_cache": true });
        let openai_body = Bytes::from(
            json!({
                "messages": [{"role": "user", "content": format!("stable {MAGIC}")}]
            })
            .to_string(),
        );
        let openai = CustomChannel.shape_request(
            openai_body,
            &mut HeaderMap::new(),
            &ctx(
                ContentGenerationKind::OpenAiChatCompletions,
                &openai_settings,
            ),
        );
        let openai: Value = serde_json::from_slice(&openai).unwrap();
        assert_eq!(
            openai["messages"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );

        let claude_body = Bytes::from(
            json!({
                "messages": [{
                    "role": "user",
                    "content": [{"type": "text", "text": format!("stable {MAGIC}")}]
                }]
            })
            .to_string(),
        );
        let disabled = CustomChannel.shape_request(
            claude_body.clone(),
            &mut HeaderMap::new(),
            &ctx(ContentGenerationKind::ClaudeMessages, &openai_settings),
        );
        assert_eq!(disabled, claude_body);

        let claude_settings = json!({ "enable_claude_magic_cache": true });
        let claude = CustomChannel.shape_request(
            claude_body,
            &mut HeaderMap::new(),
            &ctx(ContentGenerationKind::ClaudeMessages, &claude_settings),
        );
        let claude: Value = serde_json::from_slice(&claude).unwrap();
        assert_eq!(
            claude["messages"][0]["content"][0]["cache_control"]["type"],
            "ephemeral"
        );

        let fallback_settings = json!({ "claude_fable_fallbacks": "default" });
        let mut headers = HeaderMap::new();
        let fallback = CustomChannel.shape_request(
            Bytes::from_static(br#"{"model":"claude-fable-5","messages":[],"max_tokens":32}"#),
            &mut headers,
            &ctx(ContentGenerationKind::ClaudeMessages, &fallback_settings),
        );
        let fallback: Value = serde_json::from_slice(&fallback).unwrap();
        assert_eq!(fallback["fallbacks"], "default");
        assert_eq!(headers["anthropic-beta"], "server-side-fallback-2026-07-01");
    }
}
