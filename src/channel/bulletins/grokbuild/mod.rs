//! Grok Build channel — xAI Grok Build device-code/API-key access over the
//! OpenAI-like Responses API at `https://api.x.ai/v1`.

mod auth;
mod shape;
mod usage;

use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use serde_json::Value;

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::{
    Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx, PreparedRequest,
    ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, Provider};

fn is_xai_responses(op: OperationKey) -> bool {
    matches!(
        op.operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && op.kind == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses)
}

fn is_xai_responses_websocket(op: OperationKey) -> bool {
    matches!(
        op.operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && op.kind
        == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponsesWebSocket)
}

fn is_xai_image(op: OperationKey) -> bool {
    matches!(op.operation, Operation::CreateImage | Operation::EditImage)
        && op.kind == OperationKind::Provider(Provider::OpenAi)
}

pub struct GrokBuildChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for GrokBuildChannel {
    fn id(&self) -> &'static str {
        "grokbuild"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, unsupported, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(OpenAiResponsesWebSocket),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiResponsesWebSocket)),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::Gemini)),
            pass(CompactContent, pv(P::OpenAi)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let token = auth::bearer_token(ctx.secret)?.to_owned();
        let base = auth::base_url(ctx.provider_settings, ctx.secret);
        let path = auth::upstream_path(base, ctx.path);
        let websocket = crate::channel::responses_websocket::is_target(&ctx.method, ctx.path);
        let uri = join_url(base, &path, ctx.query)?;
        let headers = allow_headers(ctx.headers, &[]);
        let session_id = auth::session_id_from_body(&ctx.body);
        let accept_event_stream = !websocket
            && (ctx.method == http::Method::POST
                && path == "/responses"
                && body_streams(&ctx.body));
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        let accept = if websocket {
            auth::AcceptMode::Unset
        } else if accept_event_stream {
            auth::AcceptMode::EventStream
        } else {
            auth::AcceptMode::Json
        };
        auth::apply(&mut req, &token, accept, session_id.as_deref())?;
        if websocket {
            *req.uri_mut() = crate::channel::responses_websocket::websocket_uri(req.uri())?;
            return crate::channel::responses_websocket::prepare(req);
        }
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if is_xai_responses_websocket(ctx.op) {
            shape::shape_responses_websocket_request(body)
        } else if is_xai_responses(ctx.op) {
            shape::shape_responses_request(body)
        } else if is_xai_image(ctx.op) {
            shape::shape_image_request(body)
        } else {
            body
        }
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, secret).await
    }

    fn prepare_usage_request(
        &self,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        usage::request(secret, settings)
    }

    fn parse_usage(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        usage::parse(status, body)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for GrokBuildChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        _params: &Value,
    ) -> Result<DeviceInit, ChannelError> {
        auth::device_start(client).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        device_code: &str,
    ) -> Result<DevicePoll, ChannelError> {
        auth::device_poll(client, device_code).await
    }
}

fn body_streams(body: &Bytes) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}
