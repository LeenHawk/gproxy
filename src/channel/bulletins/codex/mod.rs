//! Codex channel — OpenAI ChatGPT-backend Responses API over OAuth2
//! (`refresh_token` grant) plus the `codex_exec` impersonation header set.
//!
//! Its stream decoder backfills an empty terminal `response.completed` output
//! from preceding items. Request shaping forces `stream`/`store`, strips
//! sampling fields, and lifts system messages into `instructions`. The inbound
//! `/v1/responses` path is rewritten to the backend `/responses`.

mod auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod headers;
mod model_metadata;
mod request;
mod request_shape;
mod token;
mod usage;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, ChannelStreamDecoder, DeviceInit,
    DevicePoll, PrepareCtx, PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{Operation, Provider};

pub struct CodexChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for CodexChannel {
    fn id(&self) -> &'static str {
        "codex"
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
            xform(
                CreateImage,
                pv(P::OpenAi),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                EditImage,
                pv(P::OpenAi),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            unsupported(CreateEmbedding, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::Gemini)),
            pass(CompactContent, pv(P::OpenAi)),
        ]
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        request::prepare(ctx)
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        request_shape::shape(body, ctx)
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(Box::new(CodexResponsesStreamDecoder::default()))
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        token::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        token::refresh(client, secret).await
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

    fn prepare_rate_limit_reset_credit_request(
        &self,
        secret: &Value,
        settings: &Value,
        idempotency_key: &str,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        usage::reset_credit_request(secret, settings, idempotency_key)
    }

    fn parse_rate_limit_reset_credit(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::RateLimitResetCreditConsumeResponse> {
        usage::parse_reset_credit(status, body)
    }

    /// Reshape the codex model catalogue into the OpenAI family canonical shape.
    /// Content ops (Responses passthrough) are returned unchanged — the codex
    /// backend already speaks OpenAI Responses, so there is nothing to reproject.
    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        match ctx.op.operation {
            Operation::ListModels => model_metadata::shape_model_list(body),
            Operation::GetModel => model_metadata::shape_model_get(body),
            _ => body,
        }
    }
}

#[derive(Default)]
struct CodexResponsesStreamDecoder {
    inner: crate::transform::stream_adapter::ResponsesStreamNormalizer,
}

impl ChannelStreamDecoder for CodexResponsesStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.inner.push(chunk)
    }

    fn finish(&mut self) -> Vec<u8> {
        self.inner.finish()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for CodexChannel {
    async fn authcode_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _params: &Value,
        redirect_uri: &str,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let (authorize_url, redirect_uri) =
            auth::authcode_start(redirect_uri, state, pkce_challenge);
        Ok(Some(AuthCodeStart {
            authorize_url,
            redirect_uri,
            extra: None,
        }))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        _extra: Option<&Value>,
    ) -> Result<Value, ChannelError> {
        auth::authcode_exchange(client, code, verifier, redirect_uri).await
    }

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
