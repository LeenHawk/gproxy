//! Google AI Studio (Gemini) channel — api key in the `?key=` query param,
//! default `https://generativelanguage.googleapis.com`.

mod auth;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{
    allow_headers_with_settings, allow_query_with_settings, build_request,
};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://generativelanguage.googleapis.com"),
    forward_headers: &[],
    // `alt` (sse) + list-models pagination (gemini wire)
    forward_query: &["alt", "pageSize", "pageToken"],
};

pub struct AiStudioChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for AiStudioChannel {
    fn id(&self) -> &'static str {
        "aistudio"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::Gemini)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::Gemini)),
            pass(ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::Gemini)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::Gemini)),
            pass(GetModel, pv(P::OpenAi)),
            pass(CountTokens, pv(P::Gemini)),
            xform(CountTokens, pv(P::Claude), CountTokens, pv(P::Gemini)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(GeminiGenerateContent)),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(StreamGenerateContent, cg(GeminiGenerateContent)),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                CreateImage,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                EditImage,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            // Gemini exposes these through its Sora-compatible OpenAI surface.
            pass(CreateVideo, pv(P::OpenAi)),
            pass(RetrieveVideo, pv(P::OpenAi)),
            pass(CreateEmbedding, pv(P::Gemini)),
            xform(
                CreateEmbedding,
                pv(P::OpenAi),
                CreateEmbedding,
                pv(P::Gemini),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
        ];
        routes.extend(responses_ws_to(cg(GeminiGenerateContent)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let api_key = common::resolve_api_key(&ctx)?;
        if matches!(
            ctx.op.operation(),
            crate::protocol::Operation::CreateVideo | crate::protocol::Operation::RetrieveVideo
        ) {
            let suffix = ctx
                .path
                .strip_prefix("/v1/videos")
                .ok_or_else(|| ChannelError::Build("invalid AI Studio video path".into()))?;
            let path = format!("/v1beta/openai/videos{suffix}");
            let uri = common::resolve_uri(&ctx, &DEFAULTS, &path, None)?;
            let headers = allow_headers_with_settings(
                ctx.headers,
                DEFAULTS.forward_headers,
                ctx.provider_settings,
            );
            let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
            // The OpenAI-compatible surface documents bearer API-key auth,
            // unlike native Gemini endpoints which use the `key` query.
            common::inject_bearer(&mut req, &api_key)?;
            return Ok(PreparedRequest::new(req));
        }
        let query = auth::apply_query(
            allow_query_with_settings(ctx.query, DEFAULTS.forward_query, ctx.provider_settings),
            &api_key,
        );
        let uri = common::resolve_uri(&ctx, &DEFAULTS, ctx.path, query.as_deref())?;
        let headers = allow_headers_with_settings(
            ctx.headers,
            DEFAULTS.forward_headers,
            ctx.provider_settings,
        );
        let req = build_request(ctx.method, uri, headers, ctx.body)?;
        Ok(PreparedRequest::new(req))
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::{HeaderMap, Method};
    use serde_json::json;

    use super::*;
    use crate::protocol::{Operation, OperationKey, Provider};

    #[test]
    fn prepares_sora_compatible_video_endpoints_with_bearer_auth() {
        let secret = json!({ "api_key": "gemini-test" });
        let settings = json!({});
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            "multipart/form-data; boundary=test".parse().unwrap(),
        );
        for (operation, method, path, expected) in [
            (
                Operation::CreateVideo,
                Method::POST,
                "/v1/videos",
                "https://generativelanguage.googleapis.com/v1beta/openai/videos",
            ),
            (
                Operation::RetrieveVideo,
                Method::GET,
                "/v1/videos/op_1",
                "https://generativelanguage.googleapis.com/v1beta/openai/videos/op_1",
            ),
        ] {
            let request = AiStudioChannel
                .prepare(PrepareCtx {
                    secret: &secret,
                    provider_settings: &settings,
                    op: OperationKey::provider(operation, Provider::OpenAi),
                    stream: false,
                    upstream_model_id: "veo-3.1-generate-preview",
                    method,
                    path,
                    query: None,
                    headers: &headers,
                    body: Bytes::new(),
                })
                .unwrap()
                .into_http()
                .unwrap();
            assert_eq!(request.uri().to_string(), expected);
            assert_eq!(request.headers()["authorization"], "Bearer gemini-test");
            assert!(request.uri().query().is_none());
        }
    }
}
