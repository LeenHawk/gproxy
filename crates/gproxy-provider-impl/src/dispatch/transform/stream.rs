use std::collections::VecDeque;
use std::io;

use bytes::Bytes;
use futures_util::stream::unfold;
use futures_util::StreamExt;

use gproxy_provider_core::{
    build_downstream_event, DownstreamContext, ProxyRequest, ProxyResponse, StreamBody,
    UpstreamPassthroughError,
};
use gproxy_protocol::claude::create_message::stream::BetaStreamEvent;
use gproxy_protocol::gemini;
use gproxy_protocol::openai;
use gproxy_protocol::sse::SseParser;
use serde_json::Value as JsonValue;

use super::super::plan::UsageKind;
use super::super::stream::{parse_gemini_stream_payload, StreamDecoder};
use super::super::usage::{
    map_usage_for_kind, ClaudeUsageState, GeminiUsageState, OpenAIResponsesUsageState,
    OpenAIUsageState, UsageState,
};
use super::super::{DispatchProvider, UpstreamOk};

pub(super) async fn transform_claude_stream<P, F, T>(
    provider: &P,
    upstream_req: ProxyRequest,
    ctx: DownstreamContext,
    usage: UsageKind,
    mut transform_factory: F,
) -> Result<ProxyResponse, UpstreamPassthroughError>
where
    P: DispatchProvider,
    F: FnMut() -> T + Send + 'static,
    T: FnMut(BetaStreamEvent) -> Vec<Bytes> + Send + 'static,
{
    let ctx_native = ctx.upstream();
    let ctx_downstream = ctx;
    let UpstreamOk { response, meta } =
        provider.call_native(upstream_req, ctx_native.clone()).await?;
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let downstream_traffic = ctx_downstream.traffic.clone();
            let downstream_meta = ctx_downstream.downstream_meta.clone();
            let downstream_trace_id = ctx_downstream.trace_id.clone();
            let response_headers = headers.clone();
            let upstream_traffic = ctx_native.traffic.clone();
            let upstream_trace_id = ctx_native.trace_id.clone();
            let upstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut usage_from_stream = None;
                let mut usage_state = match usage {
                    UsageKind::None => None,
                    _ => Some(UsageState::Claude(ClaudeUsageState::new())),
                };
                let mut parser = SseParser::new();
                let mut response_body = String::new();
                let mut response_body_raw = String::new();
                while let Some(chunk) = up_rx.recv().await {
                    response_body_raw.push_str(&String::from_utf8_lossy(&chunk));
                    response_body_raw.push('\n');
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
                let body_bytes = if response_body_raw.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body_raw))
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
            let downstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut response_body = String::new();
                while let Some(chunk) = down_rx.recv().await {
                    response_body.push_str(&String::from_utf8_lossy(&chunk));
                    response_body.push('\n');
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
                                    Err(io::Error::other(err.to_string())),
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

pub(super) async fn transform_gemini_stream<P, F, T>(
    provider: &P,
    upstream_req: ProxyRequest,
    ctx: DownstreamContext,
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
    let ctx_native = ctx.upstream();
    let ctx_downstream = ctx;
    let UpstreamOk { response, meta } =
        provider.call_native(upstream_req, ctx_native.clone()).await?;
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let downstream_traffic = ctx_downstream.traffic.clone();
            let downstream_meta = ctx_downstream.downstream_meta.clone();
            let downstream_trace_id = ctx_downstream.trace_id.clone();
            let response_headers = headers.clone();
            let upstream_traffic = ctx_native.traffic.clone();
            let upstream_trace_id = ctx_native.trace_id.clone();
            let upstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut usage_from_stream = None;
                let mut usage_state = match usage {
                    UsageKind::None => None,
                    _ => Some(UsageState::Gemini(GeminiUsageState::new())),
                };
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                let mut response_body_raw = String::new();
                while let Some(chunk) = up_rx.recv().await {
                    response_body_raw.push_str(&String::from_utf8_lossy(&chunk));
                    response_body_raw.push('\n');
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
                let body_bytes = if response_body_raw.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body_raw))
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
            let downstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut response_body = String::new();
                while let Some(chunk) = down_rx.recv().await {
                    response_body.push_str(&String::from_utf8_lossy(&chunk));
                    response_body.push('\n');
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
                                    Err(io::Error::other(err.to_string())),
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

pub(super) async fn transform_openai_chat_stream<P, F, T>(
    provider: &P,
    upstream_req: ProxyRequest,
    ctx: DownstreamContext,
    usage: UsageKind,
    mut transform_factory: F,
) -> Result<ProxyResponse, UpstreamPassthroughError>
where
    P: DispatchProvider,
    F: FnMut() -> T + Send + 'static,
    T: FnMut(openai::create_chat_completions::stream::CreateChatCompletionStreamResponse) -> Vec<Bytes>
        + Send
        + 'static,
{
    let ctx_native = ctx.upstream();
    let ctx_downstream = ctx;
    let UpstreamOk { response, meta } =
        provider.call_native(upstream_req, ctx_native.clone()).await?;
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let downstream_traffic = ctx_downstream.traffic.clone();
            let downstream_meta = ctx_downstream.downstream_meta.clone();
            let downstream_trace_id = ctx_downstream.trace_id.clone();
            let response_headers = headers.clone();
            let upstream_traffic = ctx_native.traffic.clone();
            let upstream_trace_id = ctx_native.trace_id.clone();
            let upstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut usage_from_stream = None;
                let mut usage_state = match usage {
                    UsageKind::None => None,
                    _ => Some(UsageState::OpenAI(OpenAIUsageState::new())),
                };
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                let mut response_body_raw = String::new();
                while let Some(chunk) = up_rx.recv().await {
                    response_body_raw.push_str(&String::from_utf8_lossy(&chunk));
                    response_body_raw.push('\n');
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
                let body_bytes = if response_body_raw.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body_raw))
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
            let downstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut response_body = String::new();
                while let Some(chunk) = down_rx.recv().await {
                    response_body.push_str(&String::from_utf8_lossy(&chunk));
                    response_body.push('\n');
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
                                    if data.is_empty() || data == "[DONE]" {
                                        continue;
                                    }
                                    if let Some(parsed) = parse_openai_chat_chunk(&data) {
                                        pending.extend(transform(parsed));
                                    }
                                }
                                continue;
                            }
                            Some(Err(err)) => {
                                return Some((
                                    Err(io::Error::other(err.to_string())),
                                    (upstream, decoder, transform, pending, down_tx, up_tx),
                                ))
                            }
                            None => {
                                for data in decoder.finish() {
                                    if data.is_empty() || data == "[DONE]" {
                                        continue;
                                    }
                                    if let Some(parsed) = parse_openai_chat_chunk(&data) {
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

fn parse_openai_chat_chunk(
    data: &str,
) -> Option<openai::create_chat_completions::stream::CreateChatCompletionStreamResponse> {
    if let Ok(parsed) = serde_json::from_str::<
        openai::create_chat_completions::stream::CreateChatCompletionStreamResponse,
    >(data)
    {
        return Some(parsed);
    }

    let mut value: JsonValue = serde_json::from_str(data).ok()?;
    let obj = value.as_object_mut()?;
    if !obj.contains_key("object") {
        obj.insert(
            "object".to_string(),
            JsonValue::String("chat.completion.chunk".to_string()),
        );
    }
    if !obj.contains_key("created") {
        obj.insert("created".to_string(), JsonValue::Number(0.into()));
    }
    if !obj.contains_key("id") {
        obj.insert("id".to_string(), JsonValue::String("unknown".to_string()));
    }
    if !obj.contains_key("model") {
        obj.insert("model".to_string(), JsonValue::String("unknown".to_string()));
    }
    serde_json::from_value(value).ok()
}

pub(super) async fn transform_openai_responses_stream<P, F, T>(
    provider: &P,
    upstream_req: ProxyRequest,
    ctx: DownstreamContext,
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
    let ctx_native = ctx.upstream();
    let ctx_downstream = ctx;
    let UpstreamOk { response, meta } =
        provider.call_native(upstream_req, ctx_native.clone()).await?;
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let (up_tx, mut up_rx) = tokio::sync::mpsc::channel::<Bytes>(256);
            let downstream_traffic = ctx_downstream.traffic.clone();
            let downstream_meta = ctx_downstream.downstream_meta.clone();
            let downstream_trace_id = ctx_downstream.trace_id.clone();
            let response_headers = headers.clone();
            let upstream_traffic = ctx_native.traffic.clone();
            let upstream_trace_id = ctx_native.trace_id.clone();
            let upstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut usage_from_stream = None;
                let mut usage_state = match usage {
                    UsageKind::None => None,
                    _ => Some(UsageState::OpenAIResponses(OpenAIResponsesUsageState::new())),
                };
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                let mut response_body_raw = String::new();
                while let Some(chunk) = up_rx.recv().await {
                    response_body_raw.push_str(&String::from_utf8_lossy(&chunk));
                    response_body_raw.push('\n');
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
                let body_bytes = if response_body_raw.is_empty() {
                    None
                } else {
                    Some(Bytes::from(response_body_raw))
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
            let downstream_headers = response_headers.clone();
            tokio::spawn(async move {
                let mut response_body = String::new();
                while let Some(chunk) = down_rx.recv().await {
                    response_body.push_str(&String::from_utf8_lossy(&chunk));
                    response_body.push('\n');
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
                                    Err(io::Error::other(err.to_string())),
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
