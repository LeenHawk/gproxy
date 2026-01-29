use std::collections::VecDeque;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::unfold;
use futures_util::StreamExt;
use http::header::{CONTENT_LENGTH, TRANSFER_ENCODING};
use http::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use gproxy_protocol::claude::get_model::response::GetModelResponse as ClaudeGetModelResponse;
use gproxy_protocol::claude::get_model::types::ModelInfo as ClaudeModelInfo;
use tokio::sync::mpsc;

use gproxy_provider_core::{
    build_downstream_event, build_upstream_event, CallContext, ProxyRequest, ProxyResponse,
    StreamBody, TrafficUsage, UpstreamPassthroughError, UpstreamRecordMeta,
};
use gproxy_protocol::claude::create_message::stream::BetaStreamEvent;
use gproxy_protocol::gemini;
use gproxy_protocol::openai;
use gproxy_protocol::sse::SseParser;
use gproxy_transform::count_tokens;
use gproxy_transform::generate_content;
use gproxy_transform::generate_content::claude2openai_response::stream::ClaudeToOpenAIResponseStreamState;
use gproxy_transform::generate_content::gemini2claude::stream::ClaudeToGeminiStreamState;
use gproxy_transform::get_model;
use gproxy_transform::list_models;

#[derive(Clone, Copy)]
pub enum UsageKind {
    None,
    ClaudeMessage,
    OpenAIChat,
}

pub enum DispatchPlan {
    Native { req: ProxyRequest, usage: UsageKind },
    Transform { plan: TransformPlan, usage: UsageKind },
}

pub struct UpstreamOk {
    pub response: ProxyResponse,
    pub meta: UpstreamRecordMeta,
}

pub enum TransformPlan {
    GeminiGenerate(gemini::generate_content::request::GenerateContentRequest),
    GeminiGenerateStream(gemini::stream_content::request::StreamGenerateContentRequest),
    GeminiCountTokens(gemini::count_tokens::request::CountTokensRequest),
    GeminiModelsList(gemini::list_models::request::ListModelsRequest),
    GeminiModelsGet(gemini::get_model::request::GetModelRequest),
    OpenAIResponses(openai::create_response::request::CreateResponseRequest),
    OpenAIResponsesStream(openai::create_response::request::CreateResponseRequest),
    OpenAIInputTokens(openai::count_tokens::request::InputTokenCountRequest),
    OpenAIModelsList(openai::list_models::request::ListModelsRequest),
    OpenAIModelsGet(openai::get_model::request::GetModelRequest),
}

#[async_trait]
pub trait DispatchProvider: Send + Sync {
    fn dispatch_plan(&self, req: ProxyRequest) -> DispatchPlan;

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError>;
}

pub async fn dispatch_request<P: DispatchProvider>(
    provider: &P,
    req: ProxyRequest,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match provider.dispatch_plan(req) {
        DispatchPlan::Native { req, usage } => dispatch_native(provider, req, usage, ctx).await,
        DispatchPlan::Transform { plan, usage } => dispatch_transform(provider, plan, usage, ctx).await,
    }
}

async fn dispatch_native<P: DispatchProvider>(
    provider: &P,
    req: ProxyRequest,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    let UpstreamOk { response, meta } = provider.call_native(req, ctx.clone()).await?;
    record_upstream_and_downstream(response, meta, usage, ctx).await
}

async fn dispatch_transform<P: DispatchProvider>(
    provider: &P,
    plan: TransformPlan,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    let mut ctx_native = ctx.clone();
    ctx_native.downstream_meta = None;

    match plan {
        TransformPlan::GeminiGenerate(request) => {
            let claude_request = generate_content::gemini2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeMessages(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            transform_json_response(
                upstream_recorded,
                ctx,
                generate_content::gemini2claude::response::transform_response,
            )
        }
        TransformPlan::GeminiGenerateStream(request) => {
            let request = gemini_stream_to_generate(request);
            let claude_request = generate_content::gemini2claude::request::transform_request(request);
            transform_claude_stream(
                provider,
                ProxyRequest::ClaudeMessagesStream(claude_request),
                ctx_native,
                ctx,
                usage.clone(),
                || {
                    let mut state = ClaudeToGeminiStreamState::new();
                    move |event: BetaStreamEvent| -> Vec<Bytes> {
                        state
                            .transform_event(event)
                            .into_iter()
                            .filter_map(|response| sse_json_bytes(&response))
                            .collect()
                    }
                },
            )
            .await
        }
        TransformPlan::GeminiCountTokens(request) => {
            let claude_request = count_tokens::gemini2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeCountTokens(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            transform_json_response(
                upstream_recorded,
                ctx,
                count_tokens::gemini2claude::response::transform_response,
            )
        }
        TransformPlan::GeminiModelsList(request) => {
            let claude_request = list_models::gemini2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeModelsList(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            transform_json_response(
                upstream_recorded,
                ctx,
                list_models::gemini2claude::response::transform_response,
            )
        }
        TransformPlan::GeminiModelsGet(request) => {
            let claude_request = get_model::gemini2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeModelsGet(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            match upstream_recorded {
                ProxyResponse::Json {
                    status,
                    mut headers,
                    body,
                } => {
                    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|err| {
                        UpstreamPassthroughError::service_unavailable(err.to_string())
                    })?;
                    let response: ClaudeGetModelResponse = if let Some(model) = value.get("model")
                    {
                        serde_json::from_value(model.clone()).map_err(|err| {
                            UpstreamPassthroughError::service_unavailable(err.to_string())
                        })?
                    } else {
                        serde_json::from_value(value).map_err(|err| {
                            UpstreamPassthroughError::service_unavailable(err.to_string())
                        })?
                    };
                    let mapped =
                        get_model::gemini2claude::response::transform_response(response);
                    let mapped_body = serde_json::to_vec(&mapped).map_err(|err| {
                        UpstreamPassthroughError::service_unavailable(err.to_string())
                    })?;
                    scrub_headers(&mut headers);
                    if let Some(meta) = ctx.downstream_meta {
                        let event = build_downstream_event(
                            Some(ctx.trace_id.clone()),
                            meta,
                            status,
                            &headers,
                            Some(&Bytes::from(mapped_body.clone())),
                            false,
                        );
                        ctx.traffic.record_downstream(event);
                    }
                    Ok(ProxyResponse::Json {
                        status,
                        headers,
                        body: Bytes::from(mapped_body),
                    })
                }
                ProxyResponse::Stream { .. } => Err(UpstreamPassthroughError::service_unavailable(
                    "expected json response".to_string(),
                )),
            }
        }
        TransformPlan::OpenAIResponses(request) => {
            let claude_request =
                generate_content::openai_response2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeMessages(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            transform_json_response(
                upstream_recorded,
                ctx,
                generate_content::openai_response2claude::response::transform_response,
            )
        }
        TransformPlan::OpenAIResponsesStream(request) => {
            let claude_request =
                generate_content::openai_response2claude::request::transform_request(request);
            transform_claude_stream(
                provider,
                ProxyRequest::ClaudeMessagesStream(claude_request),
                ctx_native,
                ctx,
                usage.clone(),
                || {
                    let created = now_epoch_seconds();
                    let mut state = ClaudeToOpenAIResponseStreamState::new(created);
                    move |event: BetaStreamEvent| -> Vec<Bytes> {
                        state
                            .transform_event(event)
                            .into_iter()
                            .filter_map(|response| sse_json_bytes(&response))
                            .collect()
                    }
                },
            )
            .await
        }
        TransformPlan::OpenAIInputTokens(request) => {
            let claude_request = count_tokens::openai2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeCountTokens(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            transform_json_response(
                upstream_recorded,
                ctx,
                count_tokens::openai2claude::response::transform_response,
            )
        }
        TransformPlan::OpenAIModelsList(request) => {
            let claude_request = list_models::openai2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeModelsList(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            transform_json_response(
                upstream_recorded,
                ctx,
                list_models::openai2claude::response::transform_response,
            )
        }
        TransformPlan::OpenAIModelsGet(request) => {
            let claude_request = get_model::openai2claude::request::transform_request(request);
            let UpstreamOk { response, meta } = provider
                .call_native(ProxyRequest::ClaudeModelsGet(claude_request), ctx_native)
                .await?;
            let upstream_recorded =
                record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
            transform_json_response(
                upstream_recorded,
                ctx,
                get_model::openai2claude::response::transform_response,
            )
        }
    }
}

fn transform_json_response<T, U>(
    response: ProxyResponse,
    ctx: CallContext,
    transform: fn(T) -> U,
) -> Result<ProxyResponse, UpstreamPassthroughError>
where
    T: DeserializeOwned,
    U: Serialize,
{
    match response {
        ProxyResponse::Json {
            status,
            mut headers,
            body,
        } => {
            let parsed = serde_json::from_slice::<T>(&body)
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            let mapped = transform(parsed);
            let mapped_body = serde_json::to_vec(&mapped)
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            scrub_headers(&mut headers);
            if let Some(meta) = ctx.downstream_meta {
                let event = build_downstream_event(
                    Some(ctx.trace_id.clone()),
                    meta,
                    status,
                    &headers,
                    Some(&Bytes::from(mapped_body.clone())),
                    false,
                );
                ctx.traffic.record_downstream(event);
            }
            Ok(ProxyResponse::Json {
                status,
                headers,
                body: Bytes::from(mapped_body),
            })
        }
        ProxyResponse::Stream { .. } => Err(UpstreamPassthroughError::service_unavailable(
            "expected json response".to_string(),
        )),
    }
}

async fn transform_claude_stream<P, F, T>(
    provider: &P,
    upstream_req: ProxyRequest,
    ctx_native: CallContext,
    ctx_downstream: CallContext,
    usage: UsageKind,
    mut transform_factory: F,
) -> Result<ProxyResponse, UpstreamPassthroughError>
where
    P: DispatchProvider,
    F: FnMut() -> T + Send + 'static,
    T: FnMut(BetaStreamEvent) -> Vec<Bytes> + Send + 'static,
{
    let UpstreamOk { response, meta } = provider.call_native(upstream_req, ctx_native).await?;
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let (down_tx, mut down_rx) = mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = mpsc::channel::<Bytes>(256);
            let traffic = ctx_downstream.traffic.clone();
            let downstream_meta = ctx_downstream.downstream_meta.clone();
            let trace_id = ctx_downstream.trace_id.clone();
            let response_headers = headers.clone();
            let upstream_traffic = traffic.clone();
            let upstream_trace_id = trace_id.clone();
            let upstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut usage_from_stream = None;
                let mut usage_state = match usage {
                    UsageKind::ClaudeMessage => Some(UsageState::Claude(ClaudeUsageState::new())),
                    UsageKind::OpenAIChat => Some(UsageState::OpenAI(OpenAIUsageState::new())),
                    UsageKind::None => None,
                };
                let mut parser = SseParser::new();
                let mut response_body = String::new();
                while let Some(chunk) = up_rx.recv().await {
                    for event in parser.push_bytes(&chunk) {
                        if event.data.is_empty() || event.data == "[DONE]" {
                            continue;
                        }
                        response_body.push_str(&event.data);
                        if let Some(state) = usage_state.as_mut() {
                            state.push_event(&event.data);
                        }
                    }
                }
                for event in parser.finish() {
                    if event.data.is_empty() || event.data == "[DONE]" {
                        continue;
                    }
                    response_body.push_str(&event.data);
                    if let Some(state) = usage_state.as_mut() {
                        state.push_event(&event.data);
                    }
                }
                if let Some(state) = usage_state {
                    usage_from_stream = state.finish();
                }
                let body_bytes = if response_body.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body))
                };
                let event = build_upstream_event(
                    Some(upstream_trace_id.clone()),
                    meta,
                    status,
                    &upstream_headers,
                    body_bytes.as_ref(),
                    true,
                    usage_from_stream,
                );
                upstream_traffic.record_upstream(event);
            });
            let downstream_traffic = traffic.clone();
            let downstream_trace_id = trace_id.clone();
            let downstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut parser = SseParser::new();
                let mut response_body = String::new();
                while let Some(chunk) = down_rx.recv().await {
                    for event in parser.push_bytes(&chunk) {
                        if event.data.is_empty() || event.data == "[DONE]" {
                            continue;
                        }
                        response_body.push_str(&event.data);
                    }
                }
                for event in parser.finish() {
                    if event.data.is_empty() || event.data == "[DONE]" {
                        continue;
                    }
                    response_body.push_str(&event.data);
                }
                if let Some(meta) = downstream_meta {
                    let body_bytes = if response_body.is_empty() {
                        None
                    } else {
                        Some(Bytes::from(response_body))
                    };
                    let event = build_downstream_event(
                        Some(downstream_trace_id.clone()),
                        meta,
                        status,
                        &downstream_headers,
                        body_bytes.as_ref(),
                        true,
                    );
                    downstream_traffic.record_downstream(event);
                }
            });

            let stream = unfold(
                (
                    body.stream,
                    SseParser::new(),
                    transform_factory(),
                    VecDeque::<Bytes>::new(),
                    down_tx,
                    up_tx,
                ),
                |(mut upstream, mut parser, mut transform, mut pending, down_tx, up_tx)| async move {
                    loop {
                        if let Some(item) = pending.pop_front() {
                            let _ = down_tx.send(item.clone()).await;
                            return Some((
                                Ok(item),
                                (upstream, parser, transform, pending, down_tx, up_tx),
                            ));
                        }
                        match upstream.next().await {
                            Some(Ok(bytes)) => {
                                let _ = up_tx.send(bytes.clone()).await;
                                for event in parser.push_bytes(&bytes) {
                                    if event.data.is_empty() {
                                        continue;
                                    }
                                    if let Ok(parsed) =
                                        serde_json::from_str::<BetaStreamEvent>(&event.data)
                                    {
                                        pending.extend(transform(parsed));
                                    }
                                }
                                continue;
                            }
                            Some(Err(err)) => {
                                return Some((
                                    Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
                                    (upstream, parser, transform, pending, down_tx, up_tx),
                                ))
                            }
                            None => {
                                for event in parser.finish() {
                                    if event.data.is_empty() {
                                        continue;
                                    }
                                    if let Ok(parsed) =
                                        serde_json::from_str::<BetaStreamEvent>(&event.data)
                                    {
                                        pending.extend(transform(parsed));
                                    }
                                }
                                if pending.is_empty() {
                                    return None;
                                }
                            }
                        }
                    }
                },
            );
            Ok(ProxyResponse::Stream {
                status,
                headers,
                body: StreamBody::new(body.content_type, stream),
            })
        }
        ProxyResponse::Json { .. } => Err(UpstreamPassthroughError::service_unavailable(
            "expected stream response".to_string(),
        )),
    }
}

async fn record_upstream_only(
    response: ProxyResponse,
    meta: UpstreamRecordMeta,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match &response {
        ProxyResponse::Json { status, headers, body } => {
            let usage = match usage {
                UsageKind::ClaudeMessage => extract_claude_usage_from_body(body),
                UsageKind::OpenAIChat => extract_openai_chat_usage_from_body(body),
                UsageKind::None => None,
            };
            let event = build_upstream_event(
                Some(ctx.trace_id.clone()),
                meta,
                *status,
                headers,
                Some(body),
                false,
                usage,
            );
            ctx.traffic.record_upstream(event);
            Ok(response)
        }
        ProxyResponse::Stream { .. } => Ok(response),
    }
}

async fn record_upstream_and_downstream(
    response: ProxyResponse,
    meta: UpstreamRecordMeta,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match response {
        ProxyResponse::Json { status, headers, body } => {
            let usage = match usage {
                UsageKind::ClaudeMessage => extract_claude_usage_from_body(&body),
                UsageKind::OpenAIChat => extract_openai_chat_usage_from_body(&body),
                UsageKind::None => None,
            };
            let upstream_event = build_upstream_event(
                Some(ctx.trace_id.clone()),
                meta,
                status,
                &headers,
                Some(&body),
                false,
                usage,
            );
            ctx.traffic.record_upstream(upstream_event);
            if let Some(downstream_meta) = ctx.downstream_meta {
                let downstream_event = build_downstream_event(
                    Some(ctx.trace_id.clone()),
                    downstream_meta,
                    status,
                    &headers,
                    Some(&body),
                    false,
                );
                ctx.traffic.record_downstream(downstream_event);
            }
            Ok(ProxyResponse::Json { status, headers, body })
        }
        ProxyResponse::Stream { status, headers, body } => {
            let (tx, mut rx) = mpsc::channel::<Bytes>(256);
            let traffic = ctx.traffic.clone();
            let downstream_meta = ctx.downstream_meta.clone();
            let trace_id = ctx.trace_id.clone();
            let response_headers = headers.clone();
            tokio::spawn(async move {
                let mut parser = SseParser::new();
                let mut response_body = String::new();
                let mut usage_state = match usage {
                    UsageKind::ClaudeMessage => Some(UsageState::Claude(ClaudeUsageState::new())),
                    UsageKind::OpenAIChat => Some(UsageState::OpenAI(OpenAIUsageState::new())),
                    UsageKind::None => None,
                };
                while let Some(chunk) = rx.recv().await {
                    for event in parser.push_bytes(&chunk) {
                        if event.data.is_empty() || event.data == "[DONE]" {
                            continue;
                        }
                        response_body.push_str(&event.data);
                        if let Some(state) = usage_state.as_mut() {
                            state.push_event(&event.data);
                        }
                    }
                }
                for event in parser.finish() {
                    if event.data.is_empty() || event.data == "[DONE]" {
                        continue;
                    }
                    response_body.push_str(&event.data);
                    if let Some(state) = usage_state.as_mut() {
                        state.push_event(&event.data);
                    }
                }
                let usage = usage_state.and_then(|state| state.finish());
                let body_bytes = if response_body.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body))
                };
                let upstream_event = build_upstream_event(
                    Some(trace_id.clone()),
                    meta,
                    status,
                    &response_headers,
                    body_bytes.as_ref(),
                    true,
                    usage,
                );
                traffic.record_upstream(upstream_event);
                if let Some(downstream_meta) = downstream_meta {
                    let downstream_event = build_downstream_event(
                        Some(trace_id.clone()),
                        downstream_meta,
                        status,
                        &response_headers,
                        body_bytes.as_ref(),
                        true,
                    );
                    traffic.record_downstream(downstream_event);
                }
            });
            let stream = unfold((body.stream, tx), |(mut upstream, tx)| async move {
                match upstream.next().await {
                    Some(Ok(bytes)) => {
                        let _ = tx.send(bytes.clone()).await;
                        Some((Ok(bytes), (upstream, tx)))
                    }
                    Some(Err(err)) => Some((
                        Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
                        (upstream, tx),
                    )),
                    None => None,
                }
            });
            Ok(ProxyResponse::Stream {
                status,
                headers,
                body: StreamBody::new(body.content_type, stream),
            })
        }
    }
}

fn extract_claude_usage_from_body(body: &Bytes) -> Option<TrafficUsage> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    if let Some(usage) = value.get("usage") {
        let input_tokens = usage.get("input_tokens").and_then(|v| v.as_i64());
        let output_tokens = usage.get("output_tokens").and_then(|v| v.as_i64());
        let cache_creation_input_tokens = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64());
        let cache_read_input_tokens = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64());
        if input_tokens.is_some() || output_tokens.is_some() {
            let total_tokens = match (input_tokens, output_tokens) {
                (Some(input), Some(output)) => Some(input + output),
                _ => None,
            };
            return Some(TrafficUsage {
                claude_input_tokens: input_tokens,
                claude_output_tokens: output_tokens,
                claude_total_tokens: total_tokens,
                claude_cache_creation_input_tokens: cache_creation_input_tokens,
                claude_cache_read_input_tokens: cache_read_input_tokens,
                ..Default::default()
            });
        }
    }
    if let Some(tokens) = value.get("input_tokens").and_then(|v| v.as_i64()) {
        return Some(TrafficUsage {
            claude_input_tokens: Some(tokens),
            claude_total_tokens: Some(tokens),
            ..Default::default()
        });
    }
    None
}

fn extract_openai_chat_usage_from_body(body: &Bytes) -> Option<TrafficUsage> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = value.get("usage")?;
    let prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_i64());
    let completion_tokens = usage.get("completion_tokens").and_then(|v| v.as_i64());
    let total_tokens = usage.get("total_tokens").and_then(|v| v.as_i64());
    if prompt_tokens.is_some() || completion_tokens.is_some() || total_tokens.is_some() {
        Some(TrafficUsage {
            openai_chat_prompt_tokens: prompt_tokens,
            openai_chat_completion_tokens: completion_tokens,
            openai_chat_total_tokens: total_tokens,
            ..Default::default()
        })
    } else {
        None
    }
}

struct ClaudeUsageState {
    state: gproxy_transform::stream2nostream::claude::ClaudeStreamToMessageState,
}

impl ClaudeUsageState {
    fn new() -> Self {
        Self {
            state: gproxy_transform::stream2nostream::claude::ClaudeStreamToMessageState::new(),
        }
    }

    fn push_event(&mut self, data: &str) {
        if let Ok(parsed) = serde_json::from_str::<BetaStreamEvent>(data) {
            let _ = self.state.push_event(parsed);
        }
    }

    fn finish(mut self) -> Option<TrafficUsage> {
        let message = self.state.finalize_on_eof()?;
        let input_tokens = message.usage.input_tokens as i64;
        let output_tokens = message.usage.output_tokens as i64;
        Some(TrafficUsage {
            claude_input_tokens: Some(input_tokens),
            claude_output_tokens: Some(output_tokens),
            claude_total_tokens: Some(input_tokens + output_tokens),
            claude_cache_creation_input_tokens: Some(
                message.usage.cache_creation_input_tokens as i64,
            ),
            claude_cache_read_input_tokens: Some(message.usage.cache_read_input_tokens as i64),
            ..Default::default()
        })
    }
}

struct OpenAIUsageState {
    usage: Option<TrafficUsage>,
}

impl OpenAIUsageState {
    fn new() -> Self {
        Self { usage: None }
    }

    fn push_event(&mut self, data: &str) {
        if self.usage.is_some() || data == "[DONE]" {
            return;
        }
        if let Ok(parsed) = serde_json::from_str::<
            openai::create_chat_completions::stream::CreateChatCompletionStreamResponse,
        >(data)
        {
            if let Some(stream_usage) = parsed.usage {
                self.usage = Some(TrafficUsage {
                    openai_chat_prompt_tokens: Some(stream_usage.prompt_tokens),
                    openai_chat_completion_tokens: Some(stream_usage.completion_tokens),
                    openai_chat_total_tokens: Some(stream_usage.total_tokens),
                    ..Default::default()
                });
            }
        }
    }

    fn finish(self) -> Option<TrafficUsage> {
        self.usage
    }
}

enum UsageState {
    Claude(ClaudeUsageState),
    OpenAI(OpenAIUsageState),
}

impl UsageState {
    fn push_event(&mut self, data: &str) {
        match self {
            UsageState::Claude(state) => state.push_event(data),
            UsageState::OpenAI(state) => state.push_event(data),
        }
    }

    fn finish(self) -> Option<TrafficUsage> {
        match self {
            UsageState::Claude(state) => state.finish(),
            UsageState::OpenAI(state) => state.finish(),
        }
    }
}

fn gemini_stream_to_generate(
    request: gemini::stream_content::request::StreamGenerateContentRequest,
) -> gemini::generate_content::request::GenerateContentRequest {
    gemini::generate_content::request::GenerateContentRequest {
        path: request.path,
        body: request.body,
    }
}

fn sse_json_bytes<T: Serialize>(value: &T) -> Option<Bytes> {
    let payload = serde_json::to_vec(value).ok()?;
    let mut data = Vec::with_capacity(payload.len() + 8);
    data.extend_from_slice(b"data: ");
    data.extend_from_slice(&payload);
    data.extend_from_slice(b"\n\n");
    Some(Bytes::from(data))
}

fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn scrub_headers(headers: &mut HeaderMap) {
    headers.remove(CONTENT_LENGTH);
    headers.remove(TRANSFER_ENCODING);
}
