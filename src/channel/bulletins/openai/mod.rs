//! OpenAI channel — `Authorization: Bearer`, default `https://api.openai.com`.

mod auth;

use bytes::Bytes;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{self, openai_cache};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.openai.com"),
    forward_headers: &["openai-beta", "openai-organization", "openai-project"],
    forward_query: &["after", "limit", "order", "purpose", "variant"],
};

const REALTIME_FORWARD_HEADERS: &[&str] = &[
    "openai-beta",
    "openai-alpha",
    "openai-organization",
    "openai-project",
    "x-session-id",
    "session-id",
    "thread-id",
    "originator",
];

pub struct OpenAiChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for OpenAiChannel {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn classify(
        &self,
        status: http::StatusCode,
        headers: &http::HeaderMap,
        body: &Bytes,
    ) -> crate::channel::Disposition {
        common::openai_disposition(status, headers, body)
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            // §6.3: openai-family count_tokens is served locally.
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            // Both OpenAI HTTP wire formats are native; websocket is only used
            // when the downstream request itself is Responses WebSocket.
            pass(GenerateContent, cg(OpenAiResponses)),
            xform(
                GenerateContent,
                cg(OpenAiResponsesWebSocket),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
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
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiResponsesWebSocket)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
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
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            pass(CreateEmbedding, pv(P::OpenAi)),
            pass(CreateSpeech, pv(P::OpenAi)),
            pass(CreateTranscription, pv(P::OpenAi)),
            pass(CreateTranslation, pv(P::OpenAi)),
            pass(CreateVideo, pv(P::OpenAi)),
            pass(RetrieveVideo, pv(P::OpenAi)),
            pass(ListVideos, pv(P::OpenAi)),
            pass(DeleteVideo, pv(P::OpenAi)),
            pass(DownloadVideoContent, pv(P::OpenAi)),
            pass(RemixVideo, pv(P::OpenAi)),
            pass(CreateVideoCharacter, pv(P::OpenAi)),
            pass(GetVideoCharacter, pv(P::OpenAi)),
            pass(EditVideo, pv(P::OpenAi)),
            pass(ExtendVideo, pv(P::OpenAi)),
            pass(CreateFile, pv(P::OpenAi)),
            pass(ListFiles, pv(P::OpenAi)),
            pass(RetrieveFile, pv(P::OpenAi)),
            pass(DeleteFile, pv(P::OpenAi)),
            pass(DownloadFileContent, pv(P::OpenAi)),
            xform(
                CreateEmbedding,
                pv(P::Gemini),
                CreateEmbedding,
                pv(P::OpenAi),
            ),
            pass(CompactContent, pv(P::OpenAi)),
            pass(ConnectRealtime, pv(P::OpenAi)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let responses_ws = crate::channel::responses_websocket::is_target(&ctx.method, ctx.path);
        let realtime_ws = crate::channel::realtime_websocket::is_target(&ctx.method, ctx.path);
        let (mut req, key) = if realtime_ws {
            crate::channel::realtime_websocket::build_api_key_request(
                ctx,
                &DEFAULTS,
                REALTIME_FORWARD_HEADERS,
            )?
        } else {
            common::build_request(ctx, &DEFAULTS)?
        };
        auth::apply(&mut req, &key)?;
        if responses_ws {
            crate::channel::responses_websocket::apply_beta(req.headers_mut());
            *req.uri_mut() = crate::channel::responses_websocket::websocket_uri(req.uri())?;
            return crate::channel::responses_websocket::prepare(req);
        }
        if realtime_ws {
            *req.uri_mut() = crate::channel::responses_websocket::websocket_uri(req.uri())?;
        }
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        let settings = RequestShapeSettings::from_value(ctx.settings);
        let Some(kind) = settings
            .enable_openai_magic_cache
            .then(|| openai_cache::kind_for_operation(ctx.op))
            .flatten()
        else {
            return body;
        };
        shaping::with_json_body(body, |value| {
            openai_cache::apply_magic_string_cache_breakpoints(value, kind)
        })
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::{HeaderMap, Method, StatusCode};
    use serde_json::{Value, json};

    use super::*;
    use crate::channel::routes::cg;
    use crate::protocol::{ContentGenerationKind as Kind, Operation, OperationKind};
    use crate::routing::RoutingDecision;

    fn route(operation: Operation, kind: Kind) -> RoutingDecision {
        OpenAiChannel
            .routing_table()
            .into_iter()
            .find(|(source, _)| source.operation() == operation && source.kind() == cg(kind))
            .map(|(_, decision)| decision)
            .expect("missing route")
    }

    #[test]
    fn shapes_openai_magic_cache_breakpoint_when_enabled() {
        let mut headers = HeaderMap::new();
        let settings = json!({ "enable_openai_magic_cache": true });
        let body = Bytes::from_static(
            br#"{"model":"gpt-5.6","messages":[{"role":"system","content":"stable GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH"}]}"#,
        );
        let ctx = ShapeCtx {
            op: crate::protocol::OperationKey::content_generation(
                Operation::GenerateContent,
                Kind::OpenAiChatCompletions,
            ),
            stream: false,
            status: StatusCode::OK,
            settings: &settings,
        };
        let shaped = OpenAiChannel.shape_request(body, &mut headers, &ctx);
        let value: Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(
            value["messages"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
    }

    #[test]
    fn content_defaults_use_http_openai_routes_except_websocket_source() {
        for (operation, kind) in [
            (Operation::GenerateContent, Kind::OpenAiResponses),
            (Operation::GenerateContent, Kind::OpenAiChatCompletions),
            (Operation::StreamGenerateContent, Kind::OpenAiResponses),
            (
                Operation::StreamGenerateContent,
                Kind::OpenAiChatCompletions,
            ),
            (
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket,
            ),
        ] {
            assert_eq!(route(operation, kind), RoutingDecision::Passthrough);
        }

        for (operation, kind, target_operation) in [
            (
                Operation::GenerateContent,
                Kind::OpenAiResponsesWebSocket,
                Operation::StreamGenerateContent,
            ),
            (
                Operation::GenerateContent,
                Kind::ClaudeMessages,
                Operation::GenerateContent,
            ),
            (
                Operation::GenerateContent,
                Kind::GeminiGenerateContent,
                Operation::GenerateContent,
            ),
            (
                Operation::StreamGenerateContent,
                Kind::ClaudeMessages,
                Operation::StreamGenerateContent,
            ),
            (
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
                Operation::StreamGenerateContent,
            ),
        ] {
            let RoutingDecision::TransformTo(target) = route(operation, kind) else {
                panic!("route should transform");
            };
            assert_eq!(target.operation(), target_operation);
            let target_kind = if kind == Kind::OpenAiResponsesWebSocket {
                Kind::OpenAiResponsesWebSocket
            } else {
                Kind::OpenAiChatCompletions
            };
            assert_eq!(target.kind(), OperationKind::ContentGeneration(target_kind));
        }
    }

    #[test]
    fn normal_openai_request_drops_realtime_only_headers() {
        let secret = json!({ "api_key": "sk-test" });
        let settings = json!({});
        let mut headers = HeaderMap::new();
        headers.insert("openai-beta", "feature=v1".parse().unwrap());
        headers.insert("openai-alpha", "quicksilver=v2".parse().unwrap());
        headers.insert("x-session-id", "realtime-session".parse().unwrap());
        let request = OpenAiChannel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                op: crate::protocol::OperationKey::content_generation(
                    Operation::GenerateContent,
                    Kind::OpenAiResponses,
                ),
                stream: false,
                upstream_model_id: "gpt-test",
                method: Method::POST,
                path: "/v1/responses",
                query: None,
                headers: &headers,
                body: Bytes::new(),
            })
            .unwrap()
            .into_http()
            .unwrap();

        assert_eq!(request.headers()["openai-beta"], "feature=v1");
        assert!(request.headers().get("openai-alpha").is_none());
        assert!(request.headers().get("x-session-id").is_none());
    }

    #[test]
    fn video_requests_forward_only_documented_query_parameters() {
        let secret = json!({ "api_key": "sk-test" });
        let settings = json!({});
        let headers = HeaderMap::new();
        for (operation, path, query, expected) in [
            (
                Operation::ListVideos,
                "/v1/videos",
                "after=video_1&limit=20&order=desc&key=downstream&ignored=x",
                "https://api.openai.com/v1/videos?after=video_1&limit=20&order=desc",
            ),
            (
                Operation::DownloadVideoContent,
                "/v1/videos/video_1/content",
                "variant=thumbnail&ignored=x",
                "https://api.openai.com/v1/videos/video_1/content?variant=thumbnail",
            ),
        ] {
            let request = OpenAiChannel
                .prepare(PrepareCtx {
                    secret: &secret,
                    provider_settings: &settings,
                    op: crate::protocol::OperationKey::provider(
                        operation,
                        crate::protocol::Provider::OpenAi,
                    ),
                    stream: false,
                    upstream_model_id: "",
                    method: Method::GET,
                    path,
                    query: Some(query),
                    headers: &headers,
                    body: Bytes::new(),
                })
                .unwrap()
                .into_http()
                .unwrap();
            assert_eq!(request.uri().to_string(), expected);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn prepare_responses_websocket_returns_custom_stream() {
        let secret = json!({ "api_key": "sk-test" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: crate::protocol::OperationKey::content_generation(
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket,
            ),
            stream: true,
            upstream_model_id: "gpt-test",
            method: Method::GET,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(
                br#"{"type":"response.create","model":"gpt-test","stream":true}"#,
            ),
        };

        assert!(matches!(
            OpenAiChannel.prepare(ctx).unwrap(),
            PreparedRequest::CustomStream(_)
        ));
    }
}
