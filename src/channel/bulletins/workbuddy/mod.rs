//! Tencent WorkBuddy account channel.

mod auth;
mod login;
mod shape;
mod usage;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use serde_json::Value;

use crate::channel::http_util::{
    allow_headers_with_settings, allow_query_with_settings, build_request, exact_url, join_url,
};
use crate::channel::{
    Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx, PreparedRequest,
    RefreshCtx, ShapeCtx, UsageSnapshot,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{Operation, OperationKey};

pub struct WorkBuddyChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for WorkBuddyChannel {
    fn id(&self) -> &'static str {
        "workbuddy"
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
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
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
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
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
        ];
        routes.extend(responses_ws_to(cg(OpenAiChatCompletions)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let path = upstream_path(ctx.op);
        let query = allow_query_with_settings(ctx.query, &[], ctx.provider_settings);
        let uri = match crate::channel::settings::endpoint_url(
            ctx.provider_settings,
            ctx.op,
            ctx.stream,
            ctx.upstream_model_id,
        ) {
            Some(url) => exact_url(&url, query.as_deref())?,
            None => join_url(
                auth::base_url(ctx.provider_settings),
                path,
                query.as_deref(),
            )?,
        };
        let headers = allow_headers_with_settings(ctx.headers, &[], ctx.provider_settings);
        let mut request = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut request, ctx.secret)?;
        Ok(PreparedRequest::new(request))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        shape::request(body, ctx)
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        shape::response(body, ctx)
    }

    fn credential_wide_auth(&self) -> bool {
        true
    }

    fn shares_account_quota(&self, _upstream_model_id: &str) -> bool {
        true
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        login::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: RefreshCtx<'_>,
    ) -> Result<Value, ChannelError> {
        login::refresh(client, ctx.secret, ctx.provider_settings).await
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
        _headers: &HeaderMap,
        body: &Bytes,
    ) -> Option<UsageSnapshot> {
        usage::parse(status, body)
    }

    fn describe_usage_window(
        &self,
        snapshot: &UsageSnapshot,
        index: usize,
    ) -> crate::channel::UsageWindowDescriptor {
        let window = snapshot.windows.get(index).cloned().unwrap_or_default();
        crate::channel::UsageWindowDescriptor::from_window(&window)
            .meter(crate::channel::UsageWindowMeter::Credits)
    }
}

fn upstream_path(op: OperationKey) -> &'static str {
    match op.operation() {
        Operation::ListModels => "/v3/config",
        Operation::CreateImage => "/v2/images/generations",
        Operation::EditImage => "/v2/images/edits",
        _ => "/v2/chat/completions",
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for WorkBuddyChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DeviceStartCtx<'_>,
    ) -> Result<DeviceInit, ChannelError> {
        login::device_start(client, ctx.provider_settings).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        ctx: crate::channel::DevicePollCtx<'_>,
    ) -> Result<DevicePoll, ChannelError> {
        login::device_poll(client, ctx.provider_settings, ctx.device_code).await
    }
}
