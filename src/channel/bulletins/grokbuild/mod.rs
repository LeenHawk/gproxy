//! Grok Build channel — xAI OAuth access through the CLI chat proxy.

mod auth;
mod shape;
mod usage;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use serde_json::Value;

use crate::channel::bulletins::common::xai_media;
use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::{
    Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx, PreparedRequest,
    ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, Provider};

fn is_xai_responses(op: OperationKey) -> bool {
    matches!(
        op.operation(),
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && op.kind() == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses)
}

fn is_xai_image(op: OperationKey) -> bool {
    matches!(
        op.operation(),
        Operation::CreateImage | Operation::EditImage
    ) && op.kind() == OperationKind::Provider(Provider::OpenAi)
}

fn is_xai_media(op: OperationKey) -> bool {
    op.kind() == OperationKind::Provider(Provider::OpenAi)
        && matches!(
            op.operation(),
            Operation::CreateSpeech
                | Operation::CreateTranscription
                | Operation::CreateImage
                | Operation::EditImage
                | Operation::CreateVideo
                | Operation::RetrieveVideo
                | Operation::EditVideo
                | Operation::ExtendVideo
        )
}

fn xai_media_path(operation: Operation, path: &str) -> &str {
    match operation {
        Operation::CreateSpeech => "/tts",
        Operation::CreateTranscription => "/stt",
        Operation::CreateVideo => "/videos/generations",
        _ => path.strip_prefix("/v1").unwrap_or(path),
    }
}

pub struct GrokBuildChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for GrokBuildChannel {
    fn id(&self) -> &'static str {
        "grokbuild"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, unsupported, xform};
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
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
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
            pass(CreateSpeech, pv(P::OpenAi)),
            pass(CreateTranscription, pv(P::OpenAi)),
            pass(CreateVideo, pv(P::OpenAi)),
            pass(RetrieveVideo, pv(P::OpenAi)),
            pass(EditVideo, pv(P::OpenAi)),
            pass(ExtendVideo, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::Gemini)),
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
        let media = is_xai_media(ctx.op);
        let base = if media {
            auth::xai_api_base_url(ctx.provider_settings)
        } else {
            auth::base_url(ctx.provider_settings, ctx.secret)
        };
        let path = if media {
            xai_media_path(ctx.op.operation(), ctx.path).to_owned()
        } else {
            auth::upstream_path(base, ctx.path)
        };
        let uri = match crate::channel::settings::endpoint_url_for_request(
            ctx.provider_settings,
            ctx.op,
            ctx.stream,
            ctx.upstream_model_id,
            ctx.path,
        ) {
            Some(url) => crate::channel::http_util::exact_url(&url, ctx.query)?,
            None => join_url(base, &path, ctx.query)?,
        };
        let headers = allow_headers(ctx.headers, &[]);
        let session_id = auth::session_id_from_body(&ctx.body);
        let accept_event_stream =
            ctx.method == http::Method::POST && path == "/responses" && body_streams(&ctx.body);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        let accept = if ctx.op.operation() == Operation::CreateSpeech {
            auth::AcceptMode::Audio
        } else if accept_event_stream {
            auth::AcceptMode::EventStream
        } else {
            auth::AcceptMode::Json
        };
        auth::apply(&mut req, ctx.secret, accept, session_id.as_deref())?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if is_xai_responses(ctx.op) {
            shape::shape_responses_request(body)
        } else if ctx.op.operation() == Operation::EditImage && is_xai_image(ctx.op) {
            xai_media::image_edit_request(body)
        } else if is_xai_image(ctx.op) {
            xai_media::image_request(body)
        } else if ctx.op.operation() == Operation::CreateSpeech {
            xai_media::speech_request(body)
        } else if ctx.op.operation() == Operation::CreateTranscription {
            xai_media::transcription_request(body)
        } else if matches!(
            ctx.op.operation(),
            Operation::CreateVideo | Operation::EditVideo | Operation::ExtendVideo
        ) {
            xai_media::video_request(body, ctx.op.operation())
        } else {
            body
        }
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if ctx.status.is_success()
            && matches!(
                ctx.op.operation(),
                Operation::CreateVideo
                    | Operation::RetrieveVideo
                    | Operation::EditVideo
                    | Operation::ExtendVideo
            )
        {
            xai_media::video_response(body)
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
        ctx: crate::channel::RefreshCtx<'_>,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, ctx.secret).await
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

    fn describe_usage_window(
        &self,
        snapshot: &crate::channel::UsageSnapshot,
        index: usize,
    ) -> crate::channel::UsageWindowDescriptor {
        usage::describe(snapshot, index)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for GrokBuildChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        _ctx: crate::channel::DeviceStartCtx<'_>,
    ) -> Result<DeviceInit, ChannelError> {
        auth::device_start(client).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DevicePollCtx<'_>,
    ) -> Result<DevicePoll, ChannelError> {
        auth::device_poll(client, ctx.device_code).await
    }
}

fn body_streams(body: &Bytes) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}
