use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, Uri};
use axum::response::Response;
use bytes::Bytes;
use gproxy_provider_core::{
    DownstreamContext, DownstreamRecordMeta, DownstreamTrafficEvent, ProxyRequest, ProxyResponse,
    UpstreamPassthroughError,
};
use http::header::{CONTENT_TYPE, USER_AGENT};
use tracing::{info, warn};
use uuid::Uuid;

use crate::auth::AuthError;
use crate::classify::classify_request;
use crate::core::CoreState;
use crate::error::ProxyError;

pub async fn proxy_handler(
    State(state): State<Arc<CoreState>>,
    Path((provider, path)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let Some(provider_handle) = (state.lookup)(provider.as_str()) else {
        return error_response(ProxyError::not_found("unknown provider"));
    };

    let trace_id = Uuid::new_v4().to_string();
    let auth_ctx = match state.auth.authenticate(&headers) {
        Ok(ctx) => ctx,
        Err(err) => return auth_error_response(err),
    };
    let started_at = Instant::now();
    let user_id = auth_ctx.user_id.clone();
    let key_id = auth_ctx.key_id.clone();
    let provider_id = state
        .provider_ids
        .read()
        .ok()
        .and_then(|guard| guard.get(&provider).copied());

    let request_body = body.clone();
    let classified = match classify_request(
        &method,
        &path,
        uri.query(),
        &headers,
        body,
    ) {
        Ok(req) => req,
        Err(err) => return error_response(err),
    };

    let request_for_log = classified.request.clone();
    let (operation, model) = request_operation_model(&classified.request);
    info!(
        event = "downstream_received",
        trace_id = %trace_id,
        provider = %provider,
        op = %operation,
        model = ?model,
        method = %method,
        path = %path,
        is_stream = classified.is_stream
    );
    let downstream_meta = build_downstream_meta(
        &provider,
        provider_id,
        &method,
        &path,
        uri.query(),
        &headers,
        request_body.clone(),
        &request_for_log,
        user_id.as_deref(),
        key_id.as_deref(),
    );
    let ctx = DownstreamContext {
        trace_id: trace_id.clone(),
        request_id: request_id(&headers),
        user_id: auth_ctx.user_id,
        user_key_id: auth_ctx.key_id,
        proxy: (state.proxy_resolver)(),
        traffic: state.traffic.clone(),
        downstream_meta: Some(downstream_meta),
        user_agent: headers
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string()),
    };

    let result = provider_handle.call(classified.request, ctx).await;
    let (response, event) = match result {
        Ok(response) => {
            if matches!(response, ProxyResponse::Stream { .. }) {
                info!(
                    event = "downstream_responded",
                    trace_id = %trace_id,
                    provider = %provider,
                    op = %operation,
                    status = %response_parts(&response).0.as_u16(),
                    elapsed_ms = started_at.elapsed().as_millis(),
                    is_stream = true
                );
                return proxy_response(response, &trace_id);
            }
            let event = build_downstream_event(
                &provider,
                provider_id,
                &method,
                &path,
                uri.query(),
                &headers,
                request_body,
                &request_for_log,
                &response,
                user_id.as_deref(),
                key_id.as_deref(),
                trace_id.clone(),
            );
            (response, event)
        }
        Err(err) => {
            let err_body = body_to_string(err.body.clone());
            warn!(
                event = "downstream_responded",
                trace_id = %trace_id,
                provider = %provider,
                op = %operation,
                status = %err.status.as_u16(),
                error_body = %err_body,
                elapsed_ms = started_at.elapsed().as_millis(),
                is_stream = classified.is_stream
            );
            let event = build_downstream_event_error(
                &provider,
                provider_id,
                &method,
                &path,
                uri.query(),
                &headers,
                request_body,
                &request_for_log,
                &err,
                user_id.as_deref(),
                key_id.as_deref(),
                trace_id.clone(),
            );
            return {
                state.traffic.record_downstream(event);
                passthrough_error(err)
            };
        }
    };

    state.traffic.record_downstream(event);
    let (status, _, _) = response_parts(&response);
    info!(
        event = "downstream_responded",
        trace_id = %trace_id,
        provider = %provider,
        op = %operation,
        status = %status.as_u16(),
        elapsed_ms = started_at.elapsed().as_millis(),
        is_stream = false
    );
    proxy_response(response, &trace_id)
}

fn proxy_response(response: ProxyResponse, trace_id: &str) -> Response {
    match response {
        ProxyResponse::Json {
            status,
            headers,
            body,
        } => {
            let mut resp = Response::new(Body::from(body));
            *resp.status_mut() = status;
            resp.headers_mut().extend(headers);
            if let Ok(value) = HeaderValue::from_str(trace_id) {
                resp.headers_mut().insert("x-gproxy-request-id", value);
            }
            resp
        }
        ProxyResponse::Stream {
            status,
            headers,
            body,
        } => {
            let mut resp = Response::new(Body::from_stream(body.stream));
            *resp.status_mut() = status;
            resp.headers_mut().extend(headers);
            if !resp.headers().contains_key(CONTENT_TYPE) {
                resp.headers_mut()
                    .insert(CONTENT_TYPE, HeaderValue::from_static(body.content_type));
            }
            if let Ok(value) = HeaderValue::from_str(trace_id) {
                resp.headers_mut().insert("x-gproxy-request-id", value);
            }
            resp
        }
    }
}

fn passthrough_error(err: UpstreamPassthroughError) -> Response {
    let mut resp = Response::new(Body::from(err.body));
    *resp.status_mut() = err.status;
    resp.headers_mut().extend(err.headers);
    resp
}

fn error_response(err: ProxyError) -> Response {
    let mut resp = Response::new(Body::from(err.body));
    *resp.status_mut() = err.status;
    resp
}

fn auth_error_response(err: AuthError) -> Response {
    let mut resp = Response::new(Body::from(err.body));
    *resp.status_mut() = err.status;
    resp.headers_mut().extend(err.headers);
    resp
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-request-id")
        .or_else(|| headers.get("request-id"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

#[allow(clippy::too_many_arguments)]
fn build_downstream_event(
    provider: &str,
    provider_id: Option<i64>,
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
    request: &ProxyRequest,
    response: &ProxyResponse,
    user_id: Option<&str>,
    key_id: Option<&str>,
    trace_id: String,
) -> DownstreamTrafficEvent {
    let (operation, model) = request_operation_model(request);
    let (status, resp_headers, resp_body) = response_parts(response);
    DownstreamTrafficEvent {
        trace_id: Some(trace_id),
        provider: provider.to_string(),
        provider_id,
        operation,
        model,
        user_id: user_id.and_then(|value| value.parse::<i64>().ok()),
        key_id: key_id.and_then(|value| value.parse::<i64>().ok()),
        request_method: method.to_string(),
        request_path: format!("/{}", path.trim_start_matches('/')),
        request_query: query.map(|value| value.to_string()),
        request_headers: headers_to_json(headers),
        request_body: body_to_string(body),
        response_status: status.as_u16() as i32,
        response_headers: headers_to_json(&resp_headers),
        response_body: resp_body,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_downstream_event_error(
    provider: &str,
    provider_id: Option<i64>,
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
    request: &ProxyRequest,
    error: &UpstreamPassthroughError,
    user_id: Option<&str>,
    key_id: Option<&str>,
    trace_id: String,
) -> DownstreamTrafficEvent {
    let (operation, model) = request_operation_model(request);
    DownstreamTrafficEvent {
        trace_id: Some(trace_id),
        provider: provider.to_string(),
        provider_id,
        operation,
        model,
        user_id: user_id.and_then(|value| value.parse::<i64>().ok()),
        key_id: key_id.and_then(|value| value.parse::<i64>().ok()),
        request_method: method.to_string(),
        request_path: format!("/{}", path.trim_start_matches('/')),
        request_query: query.map(|value| value.to_string()),
        request_headers: headers_to_json(headers),
        request_body: body_to_string(body),
        response_status: error.status.as_u16() as i32,
        response_headers: headers_to_json(&error.headers),
        response_body: body_to_string(error.body.clone()),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_downstream_meta(
    provider: &str,
    provider_id: Option<i64>,
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
    request: &ProxyRequest,
    user_id: Option<&str>,
    key_id: Option<&str>,
) -> DownstreamRecordMeta {
    let (operation, model) = request_operation_model(request);
    DownstreamRecordMeta {
        provider: provider.to_string(),
        provider_id,
        operation,
        model,
        user_id: user_id.and_then(|value| value.parse::<i64>().ok()),
        key_id: key_id.and_then(|value| value.parse::<i64>().ok()),
        request_method: method.to_string(),
        request_path: format!("/{}", path.trim_start_matches('/')),
        request_query: query.map(|value| value.to_string()),
        request_headers: headers_to_json(headers),
        request_body: body_to_string(body),
    }
}

fn response_parts(response: &ProxyResponse) -> (http::StatusCode, HeaderMap, String) {
    match response {
        ProxyResponse::Json {
            status,
            headers,
            body,
        } => (*status, headers.clone(), body_to_string(body.clone())),
        ProxyResponse::Stream { status, headers, .. } => {
            (*status, headers.clone(), "<stream>".to_string())
        }
    }
}

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

fn request_operation_model(request: &ProxyRequest) -> (String, Option<String>) {
    match request {
        ProxyRequest::ClaudeMessages(req) | ProxyRequest::ClaudeMessagesStream(req) => {
            ("claude.messages".to_string(), Some(model_to_string(&req.body.model)))
        }
        ProxyRequest::ClaudeCountTokens(req) => {
            ("claude.count_tokens".to_string(), Some(model_to_string(&req.body.model)))
        }
        ProxyRequest::ClaudeModelsList(_) => ("claude.models_list".to_string(), None),
        ProxyRequest::ClaudeModelsGet(req) => {
            ("claude.models_get".to_string(), Some(req.path.model_id.clone()))
        }
        ProxyRequest::GeminiGenerate(request) => {
            ("gemini.generate".to_string(), Some(request.path.model.clone()))
        }
        ProxyRequest::GeminiGenerateStream(request) => {
            ("gemini.generate_stream".to_string(), Some(request.path.model.clone()))
        }
        ProxyRequest::GeminiCountTokens(request) => {
            ("gemini.count_tokens".to_string(), Some(request.path.model.clone()))
        }
        ProxyRequest::GeminiModelsList(_) => {
            ("gemini.models_list".to_string(), None)
        }
        ProxyRequest::GeminiModelsGet(request) => {
            ("gemini.models_get".to_string(), Some(request.path.name.clone()))
        }
        ProxyRequest::OpenAIChat(req) => ("openai.chat".to_string(), Some(req.body.model.clone())),
        ProxyRequest::OpenAIChatStream(req) => ("openai.chat_stream".to_string(), Some(req.body.model.clone())),
        ProxyRequest::OpenAIResponses(req) => ("openai.responses".to_string(), Some(req.body.model.clone())),
        ProxyRequest::OpenAIResponsesStream(req) => ("openai.responses_stream".to_string(), Some(req.body.model.clone())),
        ProxyRequest::OpenAIInputTokens(req) => ("openai.input_tokens".to_string(), Some(req.body.model.clone())),
        ProxyRequest::OpenAIModelsList(_) => ("openai.models_list".to_string(), None),
        ProxyRequest::OpenAIModelsGet(req) => ("openai.models_get".to_string(), Some(req.path.model.clone())),
        ProxyRequest::OAuthStart { .. } => ("oauth".to_string(), None),
        ProxyRequest::OAuthCallback { .. } => ("oauth_callback".to_string(), None),
        ProxyRequest::Usage => ("usage".to_string(), None),
    }
}

fn model_to_string(model: &gproxy_protocol::claude::count_tokens::types::Model) -> String {
    match serde_json::to_value(model) {
        Ok(serde_json::Value::String(value)) => value,
        _ => "unknown".to_string(),
    }
}
