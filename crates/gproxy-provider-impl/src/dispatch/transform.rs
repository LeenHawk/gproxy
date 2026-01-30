use std::collections::VecDeque;
use std::io;

use bytes::Bytes;
use futures_util::stream::unfold;
use futures_util::StreamExt;
use http::header::{CONTENT_LENGTH, TRANSFER_ENCODING};
use http::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Serialize;

use gproxy_provider_core::{
    build_downstream_event, CallContext, ProxyRequest, ProxyResponse, StreamBody,
    UpstreamPassthroughError,
};
use gproxy_protocol::claude::create_message::stream::BetaStreamEvent;
use gproxy_protocol::claude::get_model::response::GetModelResponse as ClaudeGetModelResponse;
use gproxy_protocol::gemini;
use gproxy_protocol::openai;
use gproxy_protocol::sse::SseParser;
use gproxy_transform::count_tokens;
use gproxy_transform::generate_content;
use gproxy_transform::generate_content::claude2gemini::stream::GeminiToClaudeStreamState;
use gproxy_transform::generate_content::claude2openai_response::stream::ClaudeToOpenAIResponseStreamState;
use gproxy_transform::generate_content::gemini2claude::stream::ClaudeToGeminiStreamState;
use gproxy_transform::generate_content::gemini2openai_response::stream::GeminiToOpenAIResponseStreamState;
use gproxy_transform::generate_content::openai_response2claude::stream::OpenAIResponseToClaudeStreamState;
use gproxy_transform::generate_content::openai_response2gemini::stream::OpenAIResponseToGeminiStreamState;
use gproxy_transform::get_model;
use gproxy_transform::list_models;

use super::plan::{
    CountTokensPlan, GenerateContentPlan, ModelsGetPlan, ModelsListPlan, StreamContentPlan,
    TransformPlan, UsageKind,
};
use super::plan::upstream_usage_for_plan;
use super::record::record_upstream_only;
use super::stream::{
    gemini_generate_to_stream, gemini_stream_to_generate, now_epoch_seconds,
    parse_gemini_stream_payload, sse_json_bytes, StreamDecoder,
};
use super::usage::{
    ClaudeUsageState, GeminiUsageState, UsageState, map_usage_for_kind,
};
use super::{DispatchProvider, UpstreamOk};

pub(super) async fn dispatch_transform<P: DispatchProvider>(
    provider: &P,
    plan: TransformPlan,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    let mut ctx_native = ctx.clone();
    ctx_native.downstream_meta = None;
    let _downstream_usage = usage;
    let usage = upstream_usage_for_plan(&plan);

    match plan {
        TransformPlan::GenerateContent(plan) => match plan {
            GenerateContentPlan::Claude2Gemini { version, request } => {
                let gemini_request =
                    generate_content::claude2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiGenerate {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    generate_content::claude2gemini::response::transform_response,
                )
            }
            GenerateContentPlan::Claude2OpenAIResponses(request) => {
                let openai_request =
                    generate_content::claude2openai_response::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIResponses(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    generate_content::openai_response2claude::response::transform_response,
                )
            }
            GenerateContentPlan::Gemini2Claude(request) => {
                let claude_request =
                    generate_content::gemini2claude::request::transform_request(request);
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
            GenerateContentPlan::Gemini2OpenAIResponses(request) => {
                let openai_request =
                    generate_content::gemini2openai_response::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIResponses(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    generate_content::openai_response2gemini::response::transform_response,
                )
            }
            GenerateContentPlan::OpenAIResponses2Claude(request) => {
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
            GenerateContentPlan::OpenAIResponses2Gemini { version, request } => {
                let gemini_request =
                    generate_content::openai_response2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiGenerate {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    generate_content::openai_response2gemini::response::transform_response,
                )
            }
        },
        TransformPlan::StreamContent(plan) => match plan {
            StreamContentPlan::Claude2Gemini { version, request } => {
                let gemini_request =
                    generate_content::claude2gemini::request::transform_request(request);
                let stream_request = gemini_generate_to_stream(gemini_request);
                transform_gemini_stream(
                    provider,
                    ProxyRequest::GeminiGenerateStream {
                        version,
                        request: stream_request,
                    },
                    ctx_native,
                    ctx,
                    usage.clone(),
                    || {
                        let mut state = GeminiToClaudeStreamState::new();
                        move |response: gemini::generate_content::response::GenerateContentResponse| {
                            state
                                .transform_response(response)
                                .into_iter()
                                .filter_map(|event| sse_json_bytes(&event))
                                .collect()
                        }
                    },
                )
                .await
            }
            StreamContentPlan::Claude2OpenAIResponses(request) => {
                let openai_request =
                    generate_content::claude2openai_response::request::transform_request(request);
                transform_openai_responses_stream(
                    provider,
                    ProxyRequest::OpenAIResponsesStream(openai_request),
                    ctx_native,
                    ctx,
                    usage.clone(),
                    || {
                        let mut state = OpenAIResponseToClaudeStreamState::new();
                        move |event: openai::create_response::stream::ResponseStreamEvent| {
                            state
                                .transform_event(event)
                                .into_iter()
                                .filter_map(|event| sse_json_bytes(&event))
                                .collect()
                        }
                    },
                )
                .await
            }
            StreamContentPlan::Gemini2Claude(request) => {
                let request = gemini_stream_to_generate(request);
                let claude_request =
                    generate_content::gemini2claude::request::transform_request(request);
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
            StreamContentPlan::Gemini2OpenAIResponses(request) => {
                let request = gemini_stream_to_generate(request);
                let openai_request =
                    generate_content::gemini2openai_response::request::transform_request(request);
                transform_openai_responses_stream(
                    provider,
                    ProxyRequest::OpenAIResponsesStream(openai_request),
                    ctx_native,
                    ctx,
                    usage.clone(),
                    || {
                        let mut state = OpenAIResponseToGeminiStreamState::new();
                        move |event: openai::create_response::stream::ResponseStreamEvent| {
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
            StreamContentPlan::OpenAIResponses2Claude(request) => {
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
            StreamContentPlan::OpenAIResponses2Gemini { version, request } => {
                let gemini_request =
                    generate_content::openai_response2gemini::request::transform_request(request);
                let stream_request = gemini_generate_to_stream(gemini_request);
                transform_gemini_stream(
                    provider,
                    ProxyRequest::GeminiGenerateStream {
                        version,
                        request: stream_request,
                    },
                    ctx_native,
                    ctx,
                    usage.clone(),
                    || {
                        let mut state = GeminiToOpenAIResponseStreamState::new();
                        move |response: gemini::generate_content::response::GenerateContentResponse| {
                            state
                                .transform_response(response)
                                .into_iter()
                                .filter_map(|event| sse_json_bytes(&event))
                                .collect()
                        }
                    },
                )
                .await
            }
        },
        TransformPlan::CountTokens(plan) => match plan {
            CountTokensPlan::Claude2Gemini { version, request } => {
                let gemini_request = count_tokens::claude2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiCountTokens {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    count_tokens::claude2gemini::response::transform_response,
                )
            }
            CountTokensPlan::Claude2OpenAIInputTokens(request) => {
                let openai_request = count_tokens::claude2openai::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIInputTokens(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    count_tokens::claude2openai::response::transform_response,
                )
            }
            CountTokensPlan::Gemini2Claude(request) => {
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
            CountTokensPlan::Gemini2OpenAIInputTokens(request) => {
                let openai_request = count_tokens::gemini2openai::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIInputTokens(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    count_tokens::gemini2openai::response::transform_response,
                )
            }
            CountTokensPlan::OpenAIInputTokens2Claude(request) => {
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
            CountTokensPlan::OpenAIInputTokens2Gemini { version, request } => {
                let gemini_request = count_tokens::openai2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiCountTokens {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    count_tokens::openai2gemini::response::transform_response,
                )
            }
        },
        TransformPlan::ModelsList(plan) => match plan {
            ModelsListPlan::Claude2Gemini { version, request } => {
                let gemini_request = list_models::claude2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiModelsList {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    list_models::claude2gemini::response::transform_response,
                )
            }
            ModelsListPlan::Claude2OpenAI(request) => {
                let openai_request = list_models::claude2openai::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIModelsList(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    list_models::claude2openai::response::transform_response,
                )
            }
            ModelsListPlan::Gemini2Claude(request) => {
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
            ModelsListPlan::Gemini2OpenAI(request) => {
                let openai_request = list_models::gemini2openai::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIModelsList(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    list_models::gemini2openai::response::transform_response,
                )
            }
            ModelsListPlan::OpenAI2Claude(request) => {
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
            ModelsListPlan::OpenAI2Gemini { version, request } => {
                let gemini_request = list_models::openai2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiModelsList {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    list_models::openai2gemini::response::transform_response,
                )
            }
        },
        TransformPlan::ModelsGet(plan) => match plan {
            ModelsGetPlan::Claude2Gemini { version, request } => {
                let gemini_request = get_model::claude2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiModelsGet {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    get_model::claude2gemini::response::transform_response,
                )
            }
            ModelsGetPlan::Claude2OpenAI(request) => {
                let openai_request = get_model::claude2openai::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIModelsGet(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    get_model::claude2openai::response::transform_response,
                )
            }
            ModelsGetPlan::Gemini2Claude(request) => {
                let claude_request = get_model::gemini2claude::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::ClaudeModelsGet(claude_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                match upstream_recorded {
                    ProxyResponse::Json { status, mut headers, body } => {
                        let value: serde_json::Value = serde_json::from_slice(&body)
                            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
                        let response: ClaudeGetModelResponse = if let Some(model) = value.get("model") {
                            serde_json::from_value(model.clone()).map_err(|err| {
                                UpstreamPassthroughError::service_unavailable(err.to_string())
                            })?
                        } else {
                            serde_json::from_value(value).map_err(|err| {
                                UpstreamPassthroughError::service_unavailable(err.to_string())
                            })?
                        };
                        let mapped = get_model::gemini2claude::response::transform_response(response);
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
            ModelsGetPlan::Gemini2OpenAI(request) => {
                let openai_request = get_model::gemini2openai::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(ProxyRequest::OpenAIModelsGet(openai_request), ctx_native)
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    get_model::gemini2openai::response::transform_response,
                )
            }
            ModelsGetPlan::OpenAI2Claude(request) => {
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
            ModelsGetPlan::OpenAI2Gemini { version, request } => {
                let gemini_request = get_model::openai2gemini::request::transform_request(request);
                let UpstreamOk { response, meta } = provider
                    .call_native(
                        ProxyRequest::GeminiModelsGet {
                            version,
                            request: gemini_request,
                        },
                        ctx_native,
                    )
                    .await?;
                let upstream_recorded =
                    record_upstream_only(response, meta, usage.clone(), ctx.clone()).await?;
                transform_json_response(
                    upstream_recorded,
                    ctx,
                    get_model::openai2gemini::response::transform_response,
                )
            }
        },
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
        ProxyResponse::Json { status, mut headers, body } => {
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
            let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
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
                    UsageKind::None => None,
                    _ => Some(UsageState::Claude(ClaudeUsageState::new())),
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
                    usage_from_stream = map_usage_for_kind(usage, state.finish());
                }
                let body_bytes = if response_body.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body))
                };
                let event = gproxy_provider_core::build_upstream_event(
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
                                    if let Ok(parsed) = serde_json::from_str::<BetaStreamEvent>(&event.data) {
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
                                    if let Ok(parsed) = serde_json::from_str::<BetaStreamEvent>(&event.data) {
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

async fn transform_gemini_stream<P, F, T>(
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
    T: FnMut(gemini::generate_content::response::GenerateContentResponse) -> Vec<Bytes>
        + Send
        + 'static,
{
    let UpstreamOk { response, meta } = provider.call_native(upstream_req, ctx_native).await?;
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
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
                    UsageKind::None => None,
                    _ => Some(UsageState::Gemini(GeminiUsageState::new())),
                };
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                while let Some(chunk) = up_rx.recv().await {
                    for data in decoder.push(&chunk) {
                        if data.is_empty() || data == "[DONE]" {
                            continue;
                        }
                        response_body.push_str(&data);
                        if let Some(state) = usage_state.as_mut() {
                            state.push_event(&data);
                        }
                    }
                }
                for data in decoder.finish() {
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    response_body.push_str(&data);
                    if let Some(state) = usage_state.as_mut() {
                        state.push_event(&data);
                    }
                }
                if let Some(state) = usage_state {
                    usage_from_stream = map_usage_for_kind(usage, state.finish());
                }
                let body_bytes = if response_body.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body))
                };
                let event = gproxy_provider_core::build_upstream_event(
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
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                while let Some(chunk) = down_rx.recv().await {
                    for data in decoder.push(&chunk) {
                        if data.is_empty() || data == "[DONE]" {
                            continue;
                        }
                        response_body.push_str(&data);
                    }
                }
                for data in decoder.finish() {
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    response_body.push_str(&data);
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
                    StreamDecoder::new(),
                    transform_factory(),
                    VecDeque::<Bytes>::new(),
                    down_tx,
                    up_tx,
                ),
                |(mut upstream, mut decoder, mut transform, mut pending, down_tx, up_tx)| async move {
                    loop {
                        if let Some(item) = pending.pop_front() {
                            let _ = down_tx.send(item.clone()).await;
                            return Some((
                                Ok(item),
                                (upstream, decoder, transform, pending, down_tx, up_tx),
                            ));
                        }
                        match upstream.next().await {
                            Some(Ok(bytes)) => {
                                let _ = up_tx.send(bytes.clone()).await;
                                for data in decoder.push(&bytes) {
                                    if data.is_empty() {
                                        continue;
                                    }
                                    for parsed in parse_gemini_stream_payload(&data) {
                                        pending.extend(transform(parsed));
                                    }
                                }
                                continue;
                            }
                            Some(Err(err)) => {
                                return Some((
                                    Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
                                    (upstream, decoder, transform, pending, down_tx, up_tx),
                                ))
                            }
                            None => {
                                for data in decoder.finish() {
                                    if data.is_empty() {
                                        continue;
                                    }
                                    for parsed in parse_gemini_stream_payload(&data) {
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

async fn transform_openai_responses_stream<P, F, T>(
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
    T: FnMut(openai::create_response::stream::ResponseStreamEvent) -> Vec<Bytes>
        + Send
        + 'static,
{
    let UpstreamOk { response, meta } = provider.call_native(upstream_req, ctx_native).await?;
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
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
                    UsageKind::None => None,
                    _ => Some(UsageState::OpenAIResponses(
                        super::usage::OpenAIResponsesUsageState::new(),
                    )),
                };
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                while let Some(chunk) = up_rx.recv().await {
                    for data in decoder.push(&chunk) {
                        if data.is_empty() || data == "[DONE]" {
                            continue;
                        }
                        response_body.push_str(&data);
                        if let Some(state) = usage_state.as_mut() {
                            state.push_event(&data);
                        }
                    }
                }
                for data in decoder.finish() {
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    response_body.push_str(&data);
                    if let Some(state) = usage_state.as_mut() {
                        state.push_event(&data);
                    }
                }
                if let Some(state) = usage_state {
                    usage_from_stream = map_usage_for_kind(usage, state.finish());
                }
                let body_bytes = if response_body.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body))
                };
                let event = gproxy_provider_core::build_upstream_event(
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
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                while let Some(chunk) = down_rx.recv().await {
                    for data in decoder.push(&chunk) {
                        if data.is_empty() || data == "[DONE]" {
                            continue;
                        }
                        response_body.push_str(&data);
                    }
                }
                for data in decoder.finish() {
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    response_body.push_str(&data);
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
                    StreamDecoder::new(),
                    transform_factory(),
                    VecDeque::<Bytes>::new(),
                    down_tx,
                    up_tx,
                ),
                |(mut upstream, mut decoder, mut transform, mut pending, down_tx, up_tx)| async move {
                    loop {
                        if let Some(item) = pending.pop_front() {
                            let _ = down_tx.send(item.clone()).await;
                            return Some((
                                Ok(item),
                                (upstream, decoder, transform, pending, down_tx, up_tx),
                            ));
                        }
                        match upstream.next().await {
                            Some(Ok(bytes)) => {
                                let _ = up_tx.send(bytes.clone()).await;
                                for data in decoder.push(&bytes) {
                                    if data.is_empty() {
                                        continue;
                                    }
                                    if let Ok(parsed) = serde_json::from_str::<
                                        openai::create_response::stream::ResponseStreamEvent,
                                    >(&data)
                                    {
                                        pending.extend(transform(parsed));
                                    }
                                }
                                continue;
                            }
                            Some(Err(err)) => {
                                return Some((
                                    Err(io::Error::new(io::ErrorKind::Other, err.to_string())),
                                    (upstream, decoder, transform, pending, down_tx, up_tx),
                                ))
                            }
                            None => {
                                for data in decoder.finish() {
                                    if data.is_empty() {
                                        continue;
                                    }
                                    if let Ok(parsed) = serde_json::from_str::<
                                        openai::create_response::stream::ResponseStreamEvent,
                                    >(&data)
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

fn scrub_headers(headers: &mut HeaderMap) {
    headers.remove(CONTENT_LENGTH);
    headers.remove(TRANSFER_ENCODING);
}
