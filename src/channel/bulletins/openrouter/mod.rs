//! OpenRouter channel — `Authorization: Bearer`, default `https://openrouter.ai/api`.

mod auth;
mod shape;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{
    self, claude_cache_control, claude_fallback, claude_magic_cache, openai_cache,
};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://openrouter.ai/api"),
    forward_headers: &["http-referer", "x-title"],
    forward_query: &["index"],
};

pub struct OpenRouterChannel;

/// Whether `op` targets the Claude-messages content path — the only passthrough
/// route that carries a Claude-format body to shape.
fn is_claude_messages(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.kind(),
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    )
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for OpenRouterChannel {
    fn id(&self) -> &'static str {
        "openrouter"
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
            // === Rerank ===
            pass(Rerank, pv(P::OpenAi)),
            // === Images ===
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            // === Audio ===
            pass(CreateSpeech, pv(P::OpenAi)),
            pass(CreateTranscription, pv(P::OpenAi)),
            // === Video ===
            pass(CreateVideo, pv(P::OpenAi)),
            pass(RetrieveVideo, pv(P::OpenAi)),
            pass(DownloadVideoContent, pv(P::OpenAi)),
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
        let key = common::resolve_api_key(&ctx)?;
        let query = crate::channel::http_util::allow_query(ctx.query, DEFAULTS.forward_query);
        // OpenRouter's image router is `/api/v1/images`, while its public
        // OpenAPI-compatible input arrives at `/v1/images/generations`.
        let path = if matches!(
            ctx.op.operation(),
            Operation::CreateImage | Operation::EditImage
        ) {
            "/v1/images"
        } else {
            ctx.path
        };
        let uri = common::resolve_uri(&ctx, &DEFAULTS, path, query.as_deref())?;
        let headers =
            crate::channel::http_util::allow_headers(ctx.headers, DEFAULTS.forward_headers);
        let mut req = crate::channel::http_util::build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }

    /// Opt-in magic-string cache triggers for native Claude and OpenAI bodies.
    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if ctx.op.operation() == Operation::CreateVideo {
            return shaping::with_json_body(body, reshape_video_request);
        }
        if matches!(
            ctx.op.operation(),
            Operation::CreateImage | Operation::EditImage
        ) {
            return shaping::with_json_body(body, |value| {
                // OpenRouter always returns base64 image data.
                value
                    .as_object_mut()
                    .map(|object| object.remove("response_format"));
                if ctx.op.operation() == Operation::EditImage {
                    reshape_image_edit_request(value);
                }
            });
        }
        let settings = RequestShapeSettings::from_value(ctx.settings);
        if let Some(kind) = openai_cache::kind_for_operation(ctx.op) {
            return shaping::with_json_body(body, |value| {
                shape::normalize_fast_service_tier(value);
                if settings.enable_openai_magic_cache {
                    openai_cache::apply_magic_string_cache_breakpoints(value, kind);
                }
            });
        }
        if !is_claude_messages(ctx.op)
            || (!settings.enable_claude_magic_cache && settings.claude_fable_fallbacks.is_none())
        {
            return body;
        }
        shaping::with_json_body(body, |v| {
            if settings.enable_claude_magic_cache {
                claude_magic_cache::apply_magic_string_cache_control_triggers(v);
                claude_cache_control::sanitize_claude_body(v);
            }
            if let Some(fallbacks) = settings.claude_fable_fallbacks.as_ref()
                && v.get("models").is_none()
            {
                claude_fallback::apply_openrouter_fallback(v, fallbacks);
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
        if ctx.op.operation() == Operation::ListModels {
            shape::reshape_model_list(body)
        } else if matches!(
            ctx.op.operation(),
            Operation::CreateVideo | Operation::RetrieveVideo
        ) && ctx.status.is_success()
        {
            shaping::with_json_body(body, reshape_video_response)
        } else {
            shape::reshape_error(body)
        }
    }
}

fn reshape_video_request(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(seconds) = object.remove("seconds") {
        let duration = seconds
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Value::from)
            .unwrap_or(seconds);
        object.entry("duration").or_insert(duration);
    }
    let Some(reference) = object.remove("input_reference") else {
        return;
    };
    let url = reference
        .as_str()
        .map(str::to_owned)
        .or_else(|| reference.get("image_url")?.as_str().map(str::to_owned));
    if let Some(url) = url {
        object.entry("input_references").or_insert_with(
            || serde_json::json!([{"type": "image_url", "image_url": {"url": url}}]),
        );
    }
}

fn reshape_video_response(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if object.get("status").and_then(Value::as_str) == Some("pending") {
        object.insert("status".into(), Value::String("queued".into()));
    }
    if object.get("url").is_none()
        && let Some(url) = object
            .get("unsigned_urls")
            .and_then(Value::as_array)
            .and_then(|urls| urls.first())
            .and_then(Value::as_str)
            .map(str::to_owned)
    {
        object.insert("url".into(), Value::String(url));
    }
}

fn reshape_image_edit_request(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let Some(images) = object.remove("image") else {
        return;
    };
    // The image router expresses edits as generation with reference images.
    let images = match images {
        Value::Array(images) => images,
        image => vec![image],
    };
    let references: Vec<Value> = images
        .into_iter()
        .filter_map(|image| image.as_str().map(str::to_owned))
        .map(|url| serde_json::json!({ "type": "image_url", "image_url": { "url": url } }))
        .collect();
    if !references.is_empty() {
        object.insert("input_references".into(), Value::Array(references));
    }
    // OpenRouter currently has no separate mask field.
    object.remove("mask");
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderMap, StatusCode};
    use serde_json::{Value, json};

    use crate::protocol::{Operation, OperationKey};
    use crate::routing::RoutingDecision;

    fn fallback_ctx(settings: &Value) -> ShapeCtx<'_> {
        ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            status: StatusCode::OK,
            settings,
        }
    }

    #[test]
    fn rerank_defaults_to_passthrough() {
        let key = OperationKey::provider(Operation::Rerank, crate::protocol::Provider::OpenAi);
        assert_eq!(
            OpenRouterChannel
                .routing_table()
                .into_iter()
                .find(|(source, _)| *source == key),
            Some((key, RoutingDecision::Passthrough))
        );
    }

    #[test]
    fn routes_native_image_and_video_surfaces() {
        let routes = OpenRouterChannel.routing_table();
        for operation in [
            Operation::CreateImage,
            Operation::EditImage,
            Operation::CreateVideo,
            Operation::RetrieveVideo,
            Operation::DownloadVideoContent,
        ] {
            let key = OperationKey::provider(operation, crate::protocol::Provider::OpenAi);
            assert_eq!(
                routes
                    .iter()
                    .find(|(source, _)| *source == key)
                    .map(|(_, decision)| *decision),
                Some(RoutingDecision::Passthrough)
            );
        }
    }

    #[test]
    fn prepares_openrouter_image_and_video_endpoints() {
        let secret = json!({ "api_key": "or-test" });
        let settings = json!({});
        let headers = HeaderMap::new();
        for (operation, method, path, query, expected) in [
            (
                Operation::CreateImage,
                http::Method::POST,
                "/v1/images/generations",
                None,
                "https://openrouter.ai/api/v1/images",
            ),
            (
                Operation::DownloadVideoContent,
                http::Method::GET,
                "/v1/videos/job_1/content",
                Some("index=2&variant=video"),
                "https://openrouter.ai/api/v1/videos/job_1/content?index=2",
            ),
        ] {
            let request = OpenRouterChannel
                .prepare(PrepareCtx {
                    secret: &secret,
                    provider_settings: &settings,
                    op: OperationKey::provider(operation, crate::protocol::Provider::OpenAi),
                    stream: false,
                    upstream_model_id: "",
                    method,
                    path,
                    query,
                    headers: &headers,
                    body: Bytes::new(),
                })
                .unwrap()
                .into_http()
                .unwrap();
            assert_eq!(request.uri().to_string(), expected);
        }
    }

    #[test]
    fn reshapes_openai_video_fields_for_openrouter() {
        let settings = json!({});
        let ctx = ShapeCtx {
            op: OperationKey::provider(Operation::CreateVideo, crate::protocol::Provider::OpenAi),
            stream: false,
            status: StatusCode::OK,
            settings: &settings,
        };
        let mut headers = HeaderMap::new();
        let shaped = OpenRouterChannel.shape_request(
            Bytes::from_static(
                br#"{"model":"google/veo-3.1","prompt":"cat","seconds":"8","input_reference":"data:image/png;base64,AAAA"}"#,
            ),
            &mut headers,
            &ctx,
        );
        let value: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(value["duration"], 8);
        assert!(value.get("seconds").is_none());
        assert_eq!(
            value["input_references"][0]["image_url"]["url"],
            "data:image/png;base64,AAAA"
        );

        let response = OpenRouterChannel.shape_response(
            Bytes::from_static(
                br#"{"id":"job_1","status":"pending","unsigned_urls":["https://cdn/video.mp4"]}"#,
            ),
            &ctx,
        );
        let value: Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(value["status"], "queued");
        assert_eq!(value["url"], "https://cdn/video.mp4");
    }

    #[test]
    fn reshapes_image_edit_as_reference_generation() {
        let settings = json!({});
        let ctx = ShapeCtx {
            op: OperationKey::provider(Operation::EditImage, crate::protocol::Provider::OpenAi),
            stream: false,
            status: StatusCode::OK,
            settings: &settings,
        };
        let mut headers = HeaderMap::new();
        let shaped = OpenRouterChannel.shape_request(
            Bytes::from_static(
                br#"{"model":"google/gemini-image","prompt":"edit","image":["data:image/png;base64,AAAA"],"mask":"data:image/png;base64,AQID","response_format":"b64_json"}"#,
            ),
            &mut headers,
            &ctx,
        );
        let value: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(
            value["input_references"][0]["image_url"]["url"],
            "data:image/png;base64,AAAA"
        );
        assert!(value.get("image").is_none());
        assert!(value.get("mask").is_none());
        assert!(value.get("response_format").is_none());
    }

    fn openai_magic_ctx(settings: &Value) -> ShapeCtx<'_> {
        ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            stream: false,
            status: StatusCode::OK,
            settings,
        }
    }

    #[test]
    fn normalizes_openai_fast_alias_to_priority() {
        let mut headers = HeaderMap::new();
        let settings = json!({});
        let body = Bytes::from_static(
            br#"{"model":"openai/gpt-5.6","messages":[{"role":"user","content":"hi"}],"service_tier":"fast"}"#,
        );
        let shaped =
            OpenRouterChannel.shape_request(body, &mut headers, &openai_magic_ctx(&settings));
        let value: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(value["service_tier"], "priority");
    }

    #[test]
    fn shapes_openai_magic_cache_breakpoint() {
        let mut headers = HeaderMap::new();
        let settings = json!({ "enable_openai_magic_cache": true });
        let body = Bytes::from_static(
            br#"{"model":"openai/gpt-5.6","input":[{"role":"user","content":[{"type":"input_text","text":"stable GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH"}]}]}"#,
        );
        let shaped =
            OpenRouterChannel.shape_request(body, &mut headers, &openai_magic_ctx(&settings));
        let value: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(
            value["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
    }

    #[test]
    fn injects_openrouter_claude_fallback_without_anthropic_beta() {
        let mut headers = HeaderMap::new();
        let shape_settings = json!({ "claude_fable_fallbacks": ["claude-opus-4-8"] });
        let body =
            Bytes::from(r#"{"model":"anthropic/claude-sonnet-5","messages":[],"max_tokens":32}"#);
        let shaped =
            OpenRouterChannel.shape_request(body, &mut headers, &fallback_ctx(&shape_settings));

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
                op: OperationKey::content_generation(
                    Operation::GenerateContent,
                    ContentGenerationKind::ClaudeMessages,
                ),
                stream: false,
                upstream_model_id: "anthropic/claude-sonnet-5",
                method: http::Method::POST,
                path: "/v1/messages",
                query: None,
                headers: &headers,
                body: shaped,
            })
            .unwrap()
            .into_http()
            .unwrap();

        assert!(req.headers().get("anthropic-beta").is_none());
    }

    #[test]
    fn does_not_combine_fallbacks_with_openrouter_models() {
        let mut headers = HeaderMap::new();
        let settings = json!({ "claude_fable_fallbacks": ["claude-opus-4-8"] });
        let body = Bytes::from(
            r#"{"model":"anthropic/claude-fable-5","models":["anthropic/claude-fable-5","anthropic/claude-opus-4-8"],"messages":[],"max_tokens":32}"#,
        );
        let shaped = OpenRouterChannel.shape_request(body, &mut headers, &fallback_ctx(&settings));

        let v: Value = serde_json::from_slice(&shaped).unwrap();
        assert!(v.get("fallbacks").is_none());
        assert!(headers.get("anthropic-beta").is_none());
    }
}
