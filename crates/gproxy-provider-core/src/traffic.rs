use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, StatusCode};

#[derive(Debug, Clone, Default)]
pub struct DownstreamTrafficEvent {
    pub trace_id: Option<String>,
    pub provider: String,
    pub provider_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub user_id: Option<i64>,
    pub key_id: Option<i64>,

    pub request_method: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_headers: String,
    pub request_body: String,

    pub response_status: i32,
    pub response_headers: String,
    pub response_body: String,
}

#[derive(Debug, Clone, Default)]
pub struct UpstreamTrafficEvent {
    pub trace_id: Option<String>,
    pub provider: String,
    pub provider_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub credential_id: Option<i64>,

    pub request_method: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_headers: String,
    pub request_body: String,

    pub response_status: i32,
    pub response_headers: String,
    pub response_body: String,

    pub claude_input_tokens: Option<i64>,
    pub claude_output_tokens: Option<i64>,
    pub claude_total_tokens: Option<i64>,
    pub claude_cache_creation_input_tokens: Option<i64>,
    pub claude_cache_read_input_tokens: Option<i64>,

    pub gemini_prompt_tokens: Option<i64>,
    pub gemini_candidates_tokens: Option<i64>,
    pub gemini_total_tokens: Option<i64>,
    pub gemini_cached_tokens: Option<i64>,

    pub openai_chat_prompt_tokens: Option<i64>,
    pub openai_chat_completion_tokens: Option<i64>,
    pub openai_chat_total_tokens: Option<i64>,

    pub openai_responses_input_tokens: Option<i64>,
    pub openai_responses_output_tokens: Option<i64>,
    pub openai_responses_total_tokens: Option<i64>,
    pub openai_responses_input_cached_tokens: Option<i64>,
    pub openai_responses_output_reasoning_tokens: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct TrafficUsage {
    pub claude_input_tokens: Option<i64>,
    pub claude_output_tokens: Option<i64>,
    pub claude_total_tokens: Option<i64>,
    pub claude_cache_creation_input_tokens: Option<i64>,
    pub claude_cache_read_input_tokens: Option<i64>,
    pub gemini_prompt_tokens: Option<i64>,
    pub gemini_candidates_tokens: Option<i64>,
    pub gemini_total_tokens: Option<i64>,
    pub gemini_cached_tokens: Option<i64>,
    pub openai_chat_prompt_tokens: Option<i64>,
    pub openai_chat_completion_tokens: Option<i64>,
    pub openai_chat_total_tokens: Option<i64>,
    pub openai_responses_input_tokens: Option<i64>,
    pub openai_responses_output_tokens: Option<i64>,
    pub openai_responses_total_tokens: Option<i64>,
    pub openai_responses_input_cached_tokens: Option<i64>,
    pub openai_responses_output_reasoning_tokens: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DownstreamRecordMeta {
    pub provider: String,
    pub provider_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub user_id: Option<i64>,
    pub key_id: Option<i64>,
    pub request_method: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_headers: String,
    pub request_body: String,
}

#[derive(Debug, Clone)]
pub struct UpstreamRecordMeta {
    pub provider: String,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub request_method: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_headers: String,
    pub request_body: String,
}

pub fn build_downstream_event(
    trace_id: Option<String>,
    meta: DownstreamRecordMeta,
    status: StatusCode,
    headers: &HeaderMap,
    body: Option<&Bytes>,
    _is_stream: bool,
) -> DownstreamTrafficEvent {
    let response_body = body
        .map(|bytes| body_to_string(bytes.clone()))
        .unwrap_or_default();
    DownstreamTrafficEvent {
        trace_id,
        provider: meta.provider,
        provider_id: meta.provider_id,
        operation: meta.operation,
        model: meta.model,
        user_id: meta.user_id,
        key_id: meta.key_id,
        request_method: meta.request_method,
        request_path: meta.request_path,
        request_query: meta.request_query,
        request_headers: meta.request_headers,
        request_body: meta.request_body,
        response_status: status.as_u16() as i32,
        response_headers: headers_to_json(headers),
        response_body,
    }
}

pub fn build_upstream_event(
    trace_id: Option<String>,
    meta: UpstreamRecordMeta,
    status: StatusCode,
    headers: &HeaderMap,
    body: Option<&Bytes>,
    _is_stream: bool,
    usage: Option<TrafficUsage>,
) -> UpstreamTrafficEvent {
    let response_body = body
        .map(|bytes| body_to_string(bytes.clone()))
        .unwrap_or_default();
    UpstreamTrafficEvent {
        trace_id,
        provider: meta.provider,
        provider_id: meta.provider_id,
        operation: meta.operation,
        model: meta.model,
        credential_id: meta.credential_id,
        request_method: meta.request_method,
        request_path: meta.request_path,
        request_query: meta.request_query,
        request_headers: meta.request_headers,
        request_body: meta.request_body,
        response_status: status.as_u16() as i32,
        response_headers: headers_to_json(headers),
        response_body,
        claude_input_tokens: usage.as_ref().and_then(|u| u.claude_input_tokens),
        claude_output_tokens: usage.as_ref().and_then(|u| u.claude_output_tokens),
        claude_total_tokens: usage.as_ref().and_then(|u| u.claude_total_tokens),
        claude_cache_creation_input_tokens: usage
            .as_ref()
            .and_then(|u| u.claude_cache_creation_input_tokens),
        claude_cache_read_input_tokens: usage
            .as_ref()
            .and_then(|u| u.claude_cache_read_input_tokens),
        gemini_prompt_tokens: usage.as_ref().and_then(|u| u.gemini_prompt_tokens),
        gemini_candidates_tokens: usage
            .as_ref()
            .and_then(|u| u.gemini_candidates_tokens),
        gemini_total_tokens: usage.as_ref().and_then(|u| u.gemini_total_tokens),
        gemini_cached_tokens: usage.as_ref().and_then(|u| u.gemini_cached_tokens),
        openai_chat_prompt_tokens: usage.as_ref().and_then(|u| u.openai_chat_prompt_tokens),
        openai_chat_completion_tokens: usage
            .as_ref()
            .and_then(|u| u.openai_chat_completion_tokens),
        openai_chat_total_tokens: usage.as_ref().and_then(|u| u.openai_chat_total_tokens),
        openai_responses_input_tokens: usage
            .as_ref()
            .and_then(|u| u.openai_responses_input_tokens),
        openai_responses_output_tokens: usage
            .as_ref()
            .and_then(|u| u.openai_responses_output_tokens),
        openai_responses_total_tokens: usage
            .as_ref()
            .and_then(|u| u.openai_responses_total_tokens),
        openai_responses_input_cached_tokens: usage
            .as_ref()
            .and_then(|u| u.openai_responses_input_cached_tokens),
        openai_responses_output_reasoning_tokens: usage
            .as_ref()
            .and_then(|u| u.openai_responses_output_reasoning_tokens),
    }
}

pub fn record_upstream(
    sink: &SharedTrafficSink,
    trace_id: Option<String>,
    meta: UpstreamRecordMeta,
    status: StatusCode,
    headers: &HeaderMap,
    body: Option<&Bytes>,
    is_stream: bool,
) {
    let event = build_upstream_event(trace_id, meta, status, headers, body, is_stream, None);
    sink.record_upstream(event);
}

pub trait TrafficSink: Send + Sync {
    fn record_downstream(&self, event: DownstreamTrafficEvent);
    fn record_upstream(&self, event: UpstreamTrafficEvent);
}

#[derive(Debug, Default)]
pub struct NoopTrafficSink;

impl TrafficSink for NoopTrafficSink {
    fn record_downstream(&self, _event: DownstreamTrafficEvent) {}
    fn record_upstream(&self, _event: UpstreamTrafficEvent) {}
}

pub type SharedTrafficSink = Arc<dyn TrafficSink>;

fn headers_to_json(headers: &HeaderMap) -> String {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        if let Ok(value) = value.to_str() {
            map.insert(name.to_string(), serde_json::Value::String(value.to_string()));
        }
    }
    serde_json::Value::Object(map).to_string()
}

fn body_to_string(body: Bytes) -> String {
    String::from_utf8_lossy(&body).to_string()
}
