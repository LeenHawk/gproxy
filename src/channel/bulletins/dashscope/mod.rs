//! Alibaba Model Studio (DashScope) channel.

mod auth;
mod shape;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{allow_headers, allow_query, build_request};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::Operation;

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://dashscope.aliyuncs.com"),
    forward_headers: &[],
    forward_query: &[],
};

const ANTHROPIC_MESSAGES_PATH: &str = "/apps/anthropic/v1/messages";
const RERANK_PATH: &str = "/compatible-api/v1/reranks";
const IMAGE_PATH: &str = "/api/v1/services/aigc/multimodal-generation/generation";

pub struct DashScopeChannel;

fn upstream_path(ctx: &PrepareCtx<'_>) -> String {
    match ctx.op.operation() {
        Operation::GenerateContent | Operation::StreamGenerateContent
            if matches!(
                ctx.op.kind(),
                crate::protocol::OperationKind::ContentGeneration(
                    crate::protocol::ContentGenerationKind::ClaudeMessages
                )
            ) =>
        {
            ANTHROPIC_MESSAGES_PATH.into()
        }
        Operation::Rerank => RERANK_PATH.into(),
        Operation::CreateImage | Operation::EditImage => IMAGE_PATH.into(),
        _ => format!("/compatible-mode{}", ctx.path),
    }
}

fn is_image(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.operation(),
        Operation::CreateImage | Operation::EditImage
    )
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for DashScopeChannel {
    fn id(&self) -> &'static str {
        "dashscope"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiResponses),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            pass(CreateEmbedding, pv(P::OpenAi)),
            xform(
                CreateEmbedding,
                pv(P::Gemini),
                CreateEmbedding,
                pv(P::OpenAi),
            ),
            pass(Rerank, pv(P::OpenAi)),
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
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
        let path = upstream_path(&ctx);
        let key = common::resolve_api_key(&ctx)?;
        let query = allow_query(ctx.query, DEFAULTS.forward_query);
        let uri = common::resolve_uri(&ctx, &DEFAULTS, &path, query.as_deref())?;
        let headers = allow_headers(ctx.headers, DEFAULTS.forward_headers);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        match ctx.op.operation() {
            Operation::CreateImage => shape::create_request(body),
            Operation::EditImage => shape::edit_request(body),
            _ => body,
        }
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if ctx.status.is_success() && is_image(ctx.op) {
            shape::image_response(body)
        } else {
            body
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(op: crate::protocol::OperationKey, path: &str) -> http::Request<Bytes> {
        DashScopeChannel
            .prepare(PrepareCtx {
                secret: &json!({ "api_key": "test-key" }),
                provider_settings: &json!({}),
                op,
                stream: false,
                upstream_model_id: "qwen-plus",
                method: http::Method::POST,
                path,
                query: None,
                headers: &HeaderMap::new(),
                body: Bytes::from_static(b"{}"),
            })
            .unwrap()
            .into_http()
            .unwrap()
    }

    #[test]
    fn selects_dashscope_api_surfaces() {
        use crate::protocol::{ContentGenerationKind, OperationKey, Provider};

        let chat = request(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiChatCompletions,
            ),
            "/v1/chat/completions",
        );
        assert_eq!(chat.uri().path(), "/compatible-mode/v1/chat/completions");
        assert_eq!(chat.headers()["authorization"], "Bearer test-key");

        let claude = request(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            "/v1/messages",
        );
        assert_eq!(claude.uri().path(), ANTHROPIC_MESSAGES_PATH);

        let rerank = request(
            OperationKey::provider(Operation::Rerank, Provider::OpenAi),
            "/v1/rerank",
        );
        assert_eq!(rerank.uri().path(), RERANK_PATH);

        let image = request(
            OperationKey::provider(Operation::CreateImage, Provider::OpenAi),
            "/v1/images/generations",
        );
        assert_eq!(image.uri().path(), IMAGE_PATH);
    }
}
