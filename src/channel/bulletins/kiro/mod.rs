//! Kiro channel (Amazon Q / Kiro IDE) — DUAL OAuth + AWS Smithy event-stream.
//!
//! Kiro exposes no OpenAI/Claude/Gemini-compatible surface: chat goes through
//! the Smithy REST-JSON `POST /generateAssistantResponse`, whose RESPONSE is an
//! AWS binary event-stream. The upstream speaks the OpenAI Responses format, so the M2 layer
//! sees Responses on both sides — but the channel must SHAPE both directions:
//!
//!   * **request** ([`prepare`](KiroChannel::prepare)) — convert the inbound
//!     OpenAI Responses body into Kiro's `conversationState` JSON
//!     (`request::build_request_body`), lift `profileArn` to the top level, and
//!     inject the Kiro auth + IDE fingerprint headers.
//!   * **response** ([`stream_decoder`](KiroChannel::stream_decoder)) — decode the
//!     Smithy event-stream into Responses SSE (`response::KiroStreamDecoder`).
//!
//! Auth is a dual `refresh_token` grant (social vs AWS IdC) — see the internal `auth` module. This
//! is the heaviest channel; the binary frame parser lives in the channel-level
//! `aws_eventstream` module and is
//! the most-tested piece. All decode is synchronous, so the channel compiles on
//! the wasm edge target (refresh is async via [`UpstreamClient`], fine on all).

mod auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod model_list;
mod preparation;
mod request;
mod request_tools;
mod response;
mod routing;
mod sse;
mod tool_calls;
mod usage;
use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, ChannelStreamDecoder, DeviceInit,
    DevicePoll, PrepareCtx, PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::Operation;

use response::KiroStreamDecoder;

/// The Kiro region from settings (default `us-east-1`).
pub(super) fn region(settings: &Value) -> String {
    preparation::region(settings)
}

/// The management host, including any provider setting override.
pub(super) fn management_base(settings: &Value) -> String {
    preparation::management_base(settings)
}

/// AWS-JSON 1.0 content type used by both kiro.dev services.
pub(super) const AMZ_JSON: &str = "application/x-amz-json-1.0";
/// Smithy `x-amz-target`s on the management host (model-list / usage).
pub(super) const TARGET_LIST_MODELS: &str = "AmazonCodeWhispererService.ListAvailableModels";
pub(super) const TARGET_USAGE: &str = "AmazonCodeWhispererService.GetUsageLimits";
/// Runtime User-Agent the Kiro CLI sends to the management host (model-list/usage).
pub(super) const UA_MANAGEMENT: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererruntime/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";
/// Client surface reported to the Kiro/CodeWhisperer backend — sent as the
/// `origin` (chat body, usage, model-list). Captured from the real Kiro CLI
/// (`kiro-cli-chat`): it is `KIRO_CLI`, NOT v1's `AI_EDITOR`. SINGLE source of
/// truth; if a capture shows a different value, change it HERE.
pub(super) const ORIGIN: &str = "KIRO_CLI";

pub struct KiroChannel;

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub(crate) fn default_emulation() -> wreq::Emulation {
    fingerprint::default_emulation()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for KiroChannel {
    fn id(&self) -> &'static str {
        "kiro"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        routing::table()
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        preparation::prepare(ctx)
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(Box::new(KiroStreamDecoder::new()))
    }

    /// Reproject the bespoke `ListAvailableModels` body into the OpenAI family
    /// canonical model-list shape so `parse_models` reads `data[].id`. Content
    /// responses are the AWS event-stream and go through `KiroStreamDecoder`,
    /// NOT here — so every non-`ListModels` op is returned unchanged.
    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        match ctx.op.operation() {
            Operation::ListModels => model_list::to_openai(body),
            _ => body,
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
        auth::refresh(client, ctx.provider_settings, ctx.secret).await
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
impl ChannelLogin for KiroChannel {
    /// Interactive authcode+PKCE login for the AWS SSO-OIDC (builderId / idc) and
    /// external-IdP methods, dispatched on `params.auth_method` (default
    /// `builderId`). Social uses the device flow below.
    async fn authcode_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeStartCtx<'_>,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let started = auth::authcode_start(
            client,
            ctx.params,
            ctx.redirect_uri,
            ctx.state,
            ctx.pkce_challenge,
        )
        .await?;
        Ok(Some(started))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::AuthCodeExchangeCtx<'_>,
    ) -> Result<Value, ChannelError> {
        auth::authcode_exchange(client, ctx.code, ctx.verifier, ctx.redirect_uri, ctx.extra).await
    }

    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DeviceStartCtx<'_>,
    ) -> Result<DeviceInit, ChannelError> {
        auth::device_start(client, ctx.provider_settings, ctx.params).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DevicePollCtx<'_>,
    ) -> Result<DevicePoll, ChannelError> {
        auth::device_poll(client, ctx.provider_settings, ctx.device_code).await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
