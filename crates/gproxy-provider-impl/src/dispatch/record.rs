use std::io;

use bytes::Bytes;
use futures_util::stream::unfold;
use futures_util::StreamExt;
use tokio::sync::mpsc;

use gproxy_provider_core::{
    build_downstream_event, build_upstream_event, CallContext, ProxyResponse, StreamBody,
    UpstreamPassthroughError, UpstreamRecordMeta,
};
use super::stream::StreamDecoder;

use super::plan::UsageKind;
use super::usage::{extract_usage_for_kind, UsageState};

pub(super) async fn record_upstream_only(
    response: ProxyResponse,
    meta: UpstreamRecordMeta,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match &response {
        ProxyResponse::Json { status, headers, body } => {
            let usage = extract_usage_for_kind(usage, body);
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

pub(super) async fn record_upstream_and_downstream(
    response: ProxyResponse,
    meta: UpstreamRecordMeta,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match response {
        ProxyResponse::Json { status, headers, body } => {
            let usage = extract_usage_for_kind(usage, &body);
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
                let mut decoder = StreamDecoder::new();
                let mut response_body = String::new();
                let mut usage_state = match usage {
                    UsageKind::ClaudeMessage => Some(UsageState::Claude(super::usage::ClaudeUsageState::new())),
                    UsageKind::OpenAIChat => Some(UsageState::OpenAI(super::usage::OpenAIUsageState::new())),
                    UsageKind::OpenAIResponses => Some(UsageState::OpenAIResponses(
                        super::usage::OpenAIResponsesUsageState::new(),
                    )),
                    UsageKind::GeminiGenerate => {
                        Some(UsageState::Gemini(super::usage::GeminiUsageState::new()))
                    }
                    UsageKind::None => None,
                };
                while let Some(chunk) = rx.recv().await {
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
