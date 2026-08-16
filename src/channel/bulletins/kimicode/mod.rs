//! Kimi Code channel — subscription OAuth access to the managed coding API.
//!
//! This is deliberately separate from `kimiapi`: Kimi Open Platform API keys
//! use `api.moonshot.cn`, while Kimi Code subscriptions use device OAuth and
//! `api.kimi.com/coding/v1` with a stable CLI device identity. The managed API
//! natively serves OpenAI Chat Completions, OpenAI Responses, and Anthropic
//! Messages.

mod auth;
mod usage;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use serde_json::Value;

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::{
    Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx, PreparedRequest,
};
use crate::http::client::UpstreamClient;

pub struct KimiCodeChannel;

fn uses_anthropic_auth(op: crate::protocol::OperationKey) -> bool {
    use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

    matches!(
        (op.operation(), op.kind()),
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        ) | (
            Operation::CountTokens,
            OperationKind::Provider(Provider::Claude)
        )
    )
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for KimiCodeChannel {
    fn id(&self) -> &'static str {
        "kimicode"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            local(GetModel, pv(P::OpenAi)),
            local(GetModel, pv(P::Claude)),
            local(GetModel, pv(P::Gemini)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Claude)),
            pass(CountTokens, pv(P::Claude)),
            xform(CountTokens, pv(P::Gemini), CountTokens, pv(P::Claude)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(CreateEmbedding, pv(P::OpenAi)),
            xform(
                CreateEmbedding,
                pv(P::Gemini),
                CreateEmbedding,
                pv(P::OpenAi),
            ),
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
        let base = auth::base_url(ctx.provider_settings, ctx.secret);
        let path = auth::upstream_path(ctx.path);
        let uri = match crate::channel::settings::endpoint_url_for_request(
            ctx.provider_settings,
            ctx.op,
            ctx.stream,
            ctx.upstream_model_id,
            ctx.path,
        ) {
            Some(url) => crate::channel::http_util::exact_url(&url, ctx.query)?,
            None => join_url(base, path, ctx.query)?,
        };
        let headers = allow_headers(ctx.headers, &[]);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, ctx.secret, uses_anthropic_auth(ctx.op))?;
        Ok(PreparedRequest::new(req))
    }

    fn credential_wide_auth(&self) -> bool {
        true
    }

    fn shares_account_quota(&self, _upstream_model_id: &str) -> bool {
        true
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::RefreshCtx<'_>,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, ctx.secret, ctx.provider_settings).await
    }

    fn prepare_usage_request(
        &self,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<http::Request<bytes::Bytes>>, ChannelError> {
        usage::request(secret, settings)
    }

    fn parse_usage(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &bytes::Bytes,
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
impl ChannelLogin for KimiCodeChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DeviceStartCtx<'_>,
    ) -> Result<DeviceInit, ChannelError> {
        auth::device_start(client, ctx.provider_settings).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DevicePollCtx<'_>,
    ) -> Result<DevicePoll, ChannelError> {
        auth::device_poll(client, ctx.provider_settings, ctx.device_code).await
    }
}
