//! Codex channel — OpenAI ChatGPT-backend Responses API over OAuth2
//! (`refresh_token` grant) plus the `codex_exec` impersonation header set.
//!
//! Its stream decoder backfills an empty terminal `response.completed` output
//! from preceding items. Request shaping forces `stream`/`store`, strips
//! sampling fields, and lifts system messages into `instructions`. The inbound
//! `/v1/responses` path is rewritten to the backend `/responses`.

mod auth;
mod control;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod headers;
mod image_shape;
mod input_shape;
mod model_metadata;
mod request;
mod request_shape;
mod token;
mod tool_stream;
mod usage;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, ChannelStreamDecoder,
    CredentialControlOperation, CredentialControlResponse, DeviceInit, DevicePoll, PrepareCtx,
    PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::Operation;

pub struct CodexChannel;

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub(crate) fn default_emulation() -> wreq::Emulation {
    fingerprint::default_emulation()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for CodexChannel {
    fn id(&self) -> &'static str {
        "codex"
    }

    /// ChatGPT-subscription account: the OAuth token is account-wide.
    fn credential_wide_auth(&self) -> bool {
        true
    }

    /// All models draw the account 5h/weekly MAIN pool; spark
    /// (`gpt-5.3-codex-spark`) has an ADDITIONAL scoped limit on top
    /// (`GPT-5.3-Codex-Spark` in `/wham/usage`), so its own 429 stays
    /// model-scoped while a main-pool 429 blocks the whole credential.
    fn shares_account_quota(&self, upstream_model_id: &str) -> bool {
        !upstream_model_id.to_ascii_lowercase().contains("spark")
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
            pass(WebSearch, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::Gemini)),
            pass(CompactContent, pv(P::OpenAi)),
            pass(CreateRealtimeCall, pv(P::OpenAi)),
            pass(ConnectRealtime, pv(P::OpenAi)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        request::prepare(ctx)
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        match ctx.op.operation() {
            Operation::CreateImage => image_shape::create(body),
            Operation::EditImage => image_shape::edit(body),
            Operation::WebSearch | Operation::CreateRealtimeCall | Operation::CompactContent => {
                body
            }
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                request_shape::shape(body, ctx)
            }
            _ => body,
        }
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
        ctx: crate::channel::RefreshCtx<'_>,
    ) -> Result<Value, ChannelError> {
        token::refresh(client, ctx.secret).await
    }

    fn prepare_credential_control_request(
        &self,
        operation: &CredentialControlOperation,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        match operation {
            CredentialControlOperation::Usage => usage::request(secret, settings),
            CredentialControlOperation::ConsumeRateLimitResetCredit { idempotency_key } => {
                usage::reset_credit_request(secret, settings, idempotency_key)
            }
            _ => control::request(operation, secret, settings),
        }
    }

    fn parse_credential_control_response(
        &self,
        operation: &CredentialControlOperation,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<CredentialControlResponse> {
        match operation {
            CredentialControlOperation::Usage => {
                usage::parse(status, body).map(CredentialControlResponse::Usage)
            }
            CredentialControlOperation::ConsumeRateLimitResetCredit { .. } => {
                usage::parse_reset_credit(status, body)
                    .map(CredentialControlResponse::RateLimitResetCreditConsume)
            }
            _ => control::parse(status, body),
        }
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
        match ctx.op.operation() {
            Operation::ListModels => model_metadata::shape_model_list(body),
            Operation::GetModel => model_metadata::shape_model_get(body),
            _ => body,
        }
    }
}

#[derive(Default)]
struct CodexResponsesStreamDecoder {
    inner: crate::transform::stream_adapter::ResponsesStreamNormalizer,
    tools: tool_stream::ToolStreamNormalizer,
}

impl ChannelStreamDecoder for CodexResponsesStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, crate::channel::transport::ClientError> {
        let normalized = self
            .inner
            .push(chunk)
            .map_err(|error| crate::channel::transport::ClientError::Decode(error.to_string()))?;
        self.tools
            .push(&normalized)
            .map_err(|error| crate::channel::transport::ClientError::Decode(error.to_string()))
    }

    fn finish(&mut self) -> Result<Vec<u8>, crate::channel::transport::ClientError> {
        let normalized = self
            .inner
            .finish()
            .map_err(|error| crate::channel::transport::ClientError::Decode(error.to_string()))?;
        let mut output = self
            .tools
            .push(&normalized)
            .map_err(|error| crate::channel::transport::ClientError::Decode(error.to_string()))?;
        output.extend(
            self.tools.finish().map_err(|error| {
                crate::channel::transport::ClientError::Decode(error.to_string())
            })?,
        );
        Ok(output)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for CodexChannel {
    async fn authcode_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeStartCtx<'_>,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let (authorize_url, redirect_uri) =
            auth::authcode_start(ctx.redirect_uri, ctx.state, ctx.pkce_challenge);
        Ok(Some(AuthCodeStart {
            authorize_url,
            redirect_uri,
            extra: None,
        }))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeExchangeCtx<'_>,
    ) -> Result<Value, ChannelError> {
        auth::authcode_exchange(client, ctx.code, ctx.verifier, ctx.redirect_uri).await
    }

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
