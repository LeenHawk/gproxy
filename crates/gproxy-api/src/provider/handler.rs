use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_stream::try_stream;
use axum::body::Body;
use axum::extract::{Extension, Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;

use gproxy_sdk::provider::engine::{ExecuteBody, ExecuteRequest, UpstreamRequestMeta, Usage};
use gproxy_server::middleware::classify::{BufferedBodyBytes, Classification};
use gproxy_server::middleware::model_alias::ResolvedAlias;
use gproxy_server::middleware::request_model::ExtractedModel;
use gproxy_server::{AppState, OperationFamily, ProtocolKind};

use crate::auth::AuthenticatedUser;
use crate::error::HttpError;
use gproxy_storage::repository::FileRepository;

/// Proxy handler for provider-scoped routes: `/{provider}/v1/...`
pub async fn proxy(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    request: Request,
) -> Result<Response, HttpError> {
    let start = std::time::Instant::now();
    let trace_id = generate_trace_id();
    let req_method = request.method().to_string();
    let req_path = request.uri().path().to_string();
    let req_query = request.uri().query().map(String::from);
    let headers = request.headers().clone();
    let req_headers_json = headers_to_json(&headers);
    let user_key = authenticated.0;

    // Extract classification from middleware extensions
    let classification = request
        .extensions()
        .get::<Classification>()
        .cloned()
        .ok_or_else(|| HttpError::bad_request("request not classified"))?;

    // Extract model from middleware extensions
    let model = request
        .extensions()
        .get::<ExtractedModel>()
        .and_then(|m| m.0.clone());

    // Check alias resolution
    let resolved_alias = request.extensions().get::<ResolvedAlias>().cloned();
    let (effective_provider, effective_model) = if let Some(alias) = &resolved_alias
        && alias.provider_name.is_some()
    {
        (
            alias.provider_name.clone().unwrap_or(provider_name.clone()),
            alias.model_id.clone().or(model.clone()),
        )
    } else {
        (provider_name.clone(), model.clone())
    };

    // Map classification to SDK operation/protocol strings
    let operation = classification.operation;

    let req_body = build_execute_body(
        classification.operation,
        &req_path,
        req_query.as_deref(),
        buffered_request_body(&request)?,
    );

    let protocol = resolve_file_operation_protocol(
        &state,
        &effective_provider,
        operation,
        classification.protocol,
    );
    let file_plan = plan_file_operation(
        &state,
        user_key.user_id,
        user_key.id,
        &effective_provider,
        operation,
        &req_path,
        req_query.as_deref(),
    )?;

    if let Some(ref m) = effective_model
        && !is_file_operation(operation)
        && !state.check_model_permission(user_key.user_id, &effective_provider, m)
    {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    if !is_file_operation(operation)
        && let Some(ref m) = effective_model
        && let Err(_rejection) = state.check_rate_limit_request(
            user_key.user_id,
            m,
            extract_requested_total_tokens(operation, protocol, &req_body),
        )
    {
        return Err(HttpError::too_many_requests(
            "rate limit exceeded".to_string(),
        ));
    }
    if is_file_operation(operation)
        && let Err(_rejection) = state.check_rate_limit_request(
            user_key.user_id,
            &file_rate_limit_key(&effective_provider, operation),
            None,
        )
    {
        return Err(HttpError::too_many_requests(
            "rate limit exceeded".to_string(),
        ));
    }

    if let Some(FileOperationPlan::ShortCircuitJson(resp_body)) = &file_plan {
        return Ok(respond_with_local_json(
            LocalJsonResponseContext {
                state: &state,
                start,
                trace_id,
                user_id: user_key.user_id,
                user_key_id: user_key.id,
                req_method: &req_method,
                req_path: &req_path,
                req_query: req_query.as_deref(),
                req_headers_json: &req_headers_json,
                req_body: Some(&req_body),
            },
            resp_body.clone(),
        )
        .await);
    }

    let forced_credential_index = file_plan
        .as_ref()
        .and_then(FileOperationPlan::forced_credential_index);
    let deleted_file = file_plan
        .as_ref()
        .and_then(FileOperationPlan::deleted_file)
        .cloned();

    let result = match state
        .engine()
        .execute(ExecuteRequest {
            provider: effective_provider.clone(),
            operation,
            protocol,
            body: req_body.clone(),
            headers,
            model: effective_model.clone(),
            forced_credential_index,
        })
        .await
    {
        Ok(result) => result,
        Err(err) => return Err(err.into()),
    };
    let result_status = result.status;
    let result_credential_index = result.credential_index;
    let upload_body = match &result.body {
        ExecuteBody::Full(body) => Some(body.clone()),
        ExecuteBody::Stream(_) => None,
    };

    persist_claude_file_side_effects(ClaudeFileSideEffectsContext {
        state: &state,
        user_id: user_key.user_id,
        user_key_id: user_key.id,
        provider_name: &effective_provider,
        operation,
        result_status,
        result_credential_index,
        upload_body,
        deleted_file,
    })
    .await;

    // Build usage context (shared by record_usage and stream_with_usage_tracking)
    let usage_ctx = UsageRecordContext {
        state: state.clone(),
        user_id: user_key.user_id,
        user_key_id: user_key.id,
        provider_name: effective_provider.clone(),
        credential_index: Some(result.credential_index),
        precomputed_cost: result.cost,
        model: effective_model.clone(),
        billing_context: result.billing_context.clone(),
        operation,
        protocol,
        downstream_trace_id: Some(trace_id),
    };

    // Record usage via storage write channel
    if let Some(ref usage) = result.usage {
        record_usage(&usage_ctx, usage).await;
    }

    // Record upstream log
    record_upstream_log(&state, trace_id, &effective_provider, result.meta.as_ref()).await;

    let resp_status = result.status;
    let resp_headers_json = headers_to_json(&result.headers);

    let response_body = match result.body {
        ExecuteBody::Full(ref resp_body) => {
            // Record downstream log (full response available)
            let latency_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                trace_id,
                method = %req_method,
                path = %req_path,
                status = resp_status,
                latency_ms,
                "downstream"
            );
            record_downstream_log(
                &state,
                trace_id,
                user_key.user_id,
                user_key.id,
                &req_method,
                &req_path,
                req_query.as_deref(),
                &req_headers_json,
                Some(&req_body),
                Some(resp_status as i32),
                &resp_headers_json,
                Some(resp_body),
            )
            .await;
            Body::from(resp_body.clone())
        }
        ExecuteBody::Stream(stream) if classification.operation.is_stream() => {
            // For streaming: log downstream immediately (response body not captured)
            let latency_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                trace_id,
                method = %req_method,
                path = %req_path,
                status = resp_status,
                latency_ms,
                stream = true,
                "downstream"
            );
            record_downstream_log(
                &state,
                trace_id,
                user_key.user_id,
                user_key.id,
                &req_method,
                &req_path,
                req_query.as_deref(),
                &req_headers_json,
                Some(&req_body),
                Some(resp_status as i32),
                &resp_headers_json,
                None,
            )
            .await;
            Body::from_stream(stream_with_usage_tracking(usage_ctx.clone(), stream))
        }
        ExecuteBody::Stream(stream) => Body::from_stream(stream),
    };

    let mut response = Response::builder()
        .status(result.status)
        .body(response_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());

    *response.headers_mut() = result.headers;
    Ok(response)
}

/// Proxy handler for unscoped routes: `/v1/...`
pub async fn proxy_unscoped(
    State(state): State<Arc<AppState>>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    request: Request,
) -> Result<Response, HttpError> {
    let start = std::time::Instant::now();
    let trace_id = generate_trace_id();
    let req_method = request.method().to_string();
    let req_path = request.uri().path().to_string();
    let req_query = request.uri().query().map(String::from);
    let headers = request.headers().clone();
    let req_headers_json = headers_to_json(&headers);
    let user_key = authenticated.0;

    let classification = request
        .extensions()
        .get::<Classification>()
        .cloned()
        .ok_or_else(|| HttpError::bad_request("request not classified"))?;

    if classification.operation == OperationFamily::ModelList {
        let req_body = build_execute_body(
            classification.operation,
            &req_path,
            req_query.as_deref(),
            buffered_request_body(&request)?,
        );
        return Ok(respond_with_local_json(
            LocalJsonResponseContext {
                state: &state,
                start,
                trace_id,
                user_id: user_key.user_id,
                user_key_id: user_key.id,
                req_method: &req_method,
                req_path: &req_path,
                req_query: req_query.as_deref(),
                req_headers_json: &req_headers_json,
                req_body: Some(&req_body),
            },
            build_unscoped_model_list_body(
                &state,
                user_key.user_id,
                resolve_unscoped_model_list_protocol(&req_path, classification.protocol),
                &headers,
                trace_id,
            )
            .await?,
        )
        .await);
    }

    let model = request
        .extensions()
        .get::<ExtractedModel>()
        .and_then(|m| m.0.clone());

    let Some(model_name) = &model else {
        return Err(HttpError::bad_request("missing model in request"));
    };

    let req_body = build_execute_body(
        classification.operation,
        &req_path,
        req_query.as_deref(),
        buffered_request_body(&request)?,
    );

    // Resolve provider: alias → prefix → error
    let (target_provider, target_model) = if let Some(alias) = state.resolve_model_alias(model_name)
    {
        (alias.provider_name, alias.model_id)
    } else if let Some((provider, model)) = model_name.split_once('/') {
        (provider.to_string(), model.to_string())
    } else {
        return Err(HttpError::bad_request(
            "model must have provider prefix (provider/model) or match an alias",
        ));
    };

    // Check permission (whitelist)
    if !state.check_model_permission(user_key.user_id, &target_provider, &target_model) {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    let operation = classification.operation;
    let protocol = classification.protocol;
    let req_body = normalize_unscoped_request_body(operation, protocol, req_body, &target_model);

    // Check rate limit after rewriting the request body to the canonical target model.
    if let Err(_rejection) = state.check_rate_limit_request(
        user_key.user_id,
        &target_model,
        extract_requested_total_tokens(operation, protocol, &req_body),
    ) {
        return Err(HttpError::too_many_requests(
            "rate limit exceeded".to_string(),
        ));
    }

    let result = match state
        .engine()
        .execute(ExecuteRequest {
            provider: target_provider.clone(),
            operation,
            protocol,
            body: req_body.clone(),
            headers,
            model: Some(target_model.clone()),
            forced_credential_index: None,
        })
        .await
    {
        Ok(result) => result,
        Err(err) => return Err(err.into()),
    };

    let usage_ctx = UsageRecordContext {
        state: state.clone(),
        user_id: user_key.user_id,
        user_key_id: user_key.id,
        provider_name: target_provider.clone(),
        credential_index: Some(result.credential_index),
        precomputed_cost: result.cost,
        model: Some(target_model.clone()),
        billing_context: result.billing_context.clone(),
        operation,
        protocol,
        downstream_trace_id: Some(trace_id),
    };

    // Record usage via storage write channel
    if let Some(ref usage) = result.usage {
        record_usage(&usage_ctx, usage).await;
    }

    // Record upstream log
    record_upstream_log(&state, trace_id, &target_provider, result.meta.as_ref()).await;

    let resp_status = result.status;
    let resp_headers_json = headers_to_json(&result.headers);

    let response_body = match result.body {
        ExecuteBody::Full(ref resp_body) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                trace_id,
                method = %req_method,
                path = %req_path,
                status = resp_status,
                latency_ms,
                "downstream"
            );
            record_downstream_log(
                &state,
                trace_id,
                user_key.user_id,
                user_key.id,
                &req_method,
                &req_path,
                req_query.as_deref(),
                &req_headers_json,
                Some(&req_body),
                Some(resp_status as i32),
                &resp_headers_json,
                Some(resp_body),
            )
            .await;
            Body::from(resp_body.clone())
        }
        ExecuteBody::Stream(stream) if classification.operation.is_stream() => {
            let latency_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                trace_id,
                method = %req_method,
                path = %req_path,
                status = resp_status,
                latency_ms,
                stream = true,
                "downstream"
            );
            record_downstream_log(
                &state,
                trace_id,
                user_key.user_id,
                user_key.id,
                &req_method,
                &req_path,
                req_query.as_deref(),
                &req_headers_json,
                Some(&req_body),
                Some(resp_status as i32),
                &resp_headers_json,
                None,
            )
            .await;
            Body::from_stream(stream_with_usage_tracking(usage_ctx.clone(), stream))
        }
        ExecuteBody::Stream(stream) => Body::from_stream(stream),
    };

    let mut response = Response::builder()
        .status(result.status)
        .body(response_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    *response.headers_mut() = result.headers;
    Ok(response)
}

/// Proxy handler for unscoped file operations: `/v1/files/...`
///
/// File endpoints have no model in the request. Provider is resolved from
/// the `X-Provider` header.
pub async fn proxy_unscoped_files(
    State(state): State<Arc<AppState>>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    request: Request,
) -> Result<Response, HttpError> {
    let start = std::time::Instant::now();
    let trace_id = generate_trace_id();
    let req_method = request.method().to_string();
    let req_path = request.uri().path().to_string();
    let req_query = request.uri().query().map(String::from);
    let headers = request.headers().clone();
    let req_headers_json = headers_to_json(&headers);
    let user_key = authenticated.0;

    // Resolve provider from X-Provider header
    let target_provider = headers
        .get("x-provider")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .ok_or_else(|| {
            HttpError::bad_request("X-Provider header required for unscoped file operations")
        })?;

    let classification = request
        .extensions()
        .get::<Classification>()
        .cloned()
        .ok_or_else(|| HttpError::bad_request("request not classified"))?;

    let req_body = build_execute_body(
        classification.operation,
        &req_path,
        req_query.as_deref(),
        buffered_request_body(&request)?,
    );

    let operation = classification.operation;
    let protocol = resolve_file_operation_protocol(
        &state,
        &target_provider,
        operation,
        classification.protocol,
    );
    let file_plan = plan_file_operation(
        &state,
        user_key.user_id,
        user_key.id,
        &target_provider,
        operation,
        &req_path,
        req_query.as_deref(),
    )?;

    if let Err(_rejection) = state.check_rate_limit_request(
        user_key.user_id,
        &file_rate_limit_key(&target_provider, operation),
        None,
    ) {
        return Err(HttpError::too_many_requests(
            "rate limit exceeded".to_string(),
        ));
    }

    if let Some(FileOperationPlan::ShortCircuitJson(resp_body)) = &file_plan {
        return Ok(respond_with_local_json(
            LocalJsonResponseContext {
                state: &state,
                start,
                trace_id,
                user_id: user_key.user_id,
                user_key_id: user_key.id,
                req_method: &req_method,
                req_path: &req_path,
                req_query: req_query.as_deref(),
                req_headers_json: &req_headers_json,
                req_body: Some(&req_body),
            },
            resp_body.clone(),
        )
        .await);
    }

    let forced_credential_index = file_plan
        .as_ref()
        .and_then(FileOperationPlan::forced_credential_index);
    let deleted_file = file_plan
        .as_ref()
        .and_then(FileOperationPlan::deleted_file)
        .cloned();

    let result = state
        .engine()
        .execute(ExecuteRequest {
            provider: target_provider.clone(),
            operation,
            protocol,
            body: req_body.clone(),
            headers,
            model: None,
            forced_credential_index,
        })
        .await?;
    let result_status = result.status;
    let result_credential_index = result.credential_index;
    let upload_body = match &result.body {
        ExecuteBody::Full(body) => Some(body.clone()),
        ExecuteBody::Stream(_) => None,
    };

    persist_claude_file_side_effects(ClaudeFileSideEffectsContext {
        state: &state,
        user_id: user_key.user_id,
        user_key_id: user_key.id,
        provider_name: &target_provider,
        operation,
        result_status,
        result_credential_index,
        upload_body,
        deleted_file,
    })
    .await;

    // Record usage via storage write channel
    if let Some(ref usage) = result.usage {
        let usage_ctx = UsageRecordContext {
            state: state.clone(),
            user_id: user_key.user_id,
            user_key_id: user_key.id,
            provider_name: target_provider.clone(),
            credential_index: Some(result.credential_index),
            precomputed_cost: result.cost,
            model: None,
            billing_context: result.billing_context.clone(),
            operation,
            protocol,
            downstream_trace_id: Some(trace_id),
        };
        record_usage(&usage_ctx, usage).await;
    }

    // Record upstream log
    record_upstream_log(&state, trace_id, &target_provider, result.meta.as_ref()).await;

    let resp_status = result.status;
    let resp_headers_json = headers_to_json(&result.headers);

    let response_body = match result.body {
        ExecuteBody::Full(ref resp_body) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            tracing::info!(
                trace_id,
                method = %req_method,
                path = %req_path,
                status = resp_status,
                latency_ms,
                "downstream"
            );
            record_downstream_log(
                &state,
                trace_id,
                user_key.user_id,
                user_key.id,
                &req_method,
                &req_path,
                req_query.as_deref(),
                &req_headers_json,
                Some(&req_body),
                Some(resp_status as i32),
                &resp_headers_json,
                Some(resp_body),
            )
            .await;
            Body::from(resp_body.clone())
        }
        ExecuteBody::Stream(stream) => Body::from_stream(stream),
    };

    let mut response = Response::builder()
        .status(result.status)
        .body(response_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    *response.headers_mut() = result.headers;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum FileOperationPlan {
    ShortCircuitJson(Vec<u8>),
    Upstream {
        forced_credential_index: Option<usize>,
        deleted_file: Option<gproxy_server::MemoryUserCredentialFile>,
    },
}

impl FileOperationPlan {
    fn forced_credential_index(&self) -> Option<usize> {
        match self {
            Self::ShortCircuitJson(_) => None,
            Self::Upstream {
                forced_credential_index,
                ..
            } => *forced_credential_index,
        }
    }

    fn deleted_file(&self) -> Option<&gproxy_server::MemoryUserCredentialFile> {
        match self {
            Self::ShortCircuitJson(_) => None,
            Self::Upstream { deleted_file, .. } => deleted_file.as_ref(),
        }
    }
}

struct LocalJsonResponseContext<'a> {
    state: &'a AppState,
    start: std::time::Instant,
    trace_id: i64,
    user_id: i64,
    user_key_id: i64,
    req_method: &'a str,
    req_path: &'a str,
    req_query: Option<&'a str>,
    req_headers_json: &'a str,
    req_body: Option<&'a Vec<u8>>,
}

struct ClaudeFileSideEffectsContext<'a> {
    state: &'a AppState,
    user_id: i64,
    user_key_id: i64,
    provider_name: &'a str,
    operation: OperationFamily,
    result_status: u16,
    result_credential_index: usize,
    upload_body: Option<Vec<u8>>,
    deleted_file: Option<gproxy_server::MemoryUserCredentialFile>,
}

struct ClaudeFileAccess {
    record: gproxy_server::MemoryUserCredentialFile,
    metadata: Option<gproxy_sdk::protocol::claude::types::FileMetadata>,
    forced_credential_index: usize,
}

struct ClaudeFileListQuery {
    after_id: Option<String>,
    before_id: Option<String>,
    limit: usize,
}

fn is_file_operation(operation: OperationFamily) -> bool {
    matches!(
        operation,
        OperationFamily::FileUpload
            | OperationFamily::FileList
            | OperationFamily::FileGet
            | OperationFamily::FileContent
            | OperationFamily::FileDelete
    )
}

fn file_rate_limit_key(provider_name: &str, operation: OperationFamily) -> String {
    format!("file/{provider_name}/{operation}")
}

struct AggregatedModelListEntry {
    provider_name: String,
}

fn is_claude_file_provider(state: &AppState, provider_name: &str) -> bool {
    state
        .provider_channel_for_name(provider_name)
        .as_deref()
        .is_some_and(|channel| matches!(channel, "anthropic" | "claudecode"))
}

fn resolve_file_operation_protocol(
    state: &AppState,
    provider_name: &str,
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> ProtocolKind {
    if is_file_operation(operation) && is_claude_file_provider(state, provider_name) {
        ProtocolKind::Claude
    } else {
        protocol
    }
}

fn parse_claude_file_list_query(query: Option<&str>) -> ClaudeFileListQuery {
    let mut after_id = None;
    let mut before_id = None;
    let mut limit = 20usize;

    if let Some(raw_query) = query {
        for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            match key.as_ref() {
                "after_id" if !value.is_empty() => after_id = Some(value.into_owned()),
                "before_id" if !value.is_empty() => before_id = Some(value.into_owned()),
                "limit" => {
                    if let Ok(parsed) = value.parse::<usize>() {
                        limit = parsed.clamp(1, 1000);
                    }
                }
                _ => {}
            }
        }
    }

    ClaudeFileListQuery {
        after_id,
        before_id,
        limit,
    }
}

fn parse_claude_timestamp_ms(raw: &str) -> i64 {
    time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339)
        .map(|dt| dt.unix_timestamp_nanos() as i64 / 1_000_000)
        .unwrap_or_default()
}

fn resolve_unscoped_model_list_protocol(req_path: &str, classified: ProtocolKind) -> ProtocolKind {
    if req_path.starts_with("/v1beta/") {
        ProtocolKind::Gemini
    } else {
        classified
    }
}

fn prefixed_model_id(provider_name: &str, model_id: &str) -> String {
    format!("{provider_name}/{model_id}")
}

async fn collect_unscoped_authorized_models(
    state: &AppState,
    user_id: i64,
) -> Result<Vec<AggregatedModelListEntry>, HttpError> {
    let mut providers: Vec<AggregatedModelListEntry> = state
        .storage()
        .list_providers(&gproxy_storage::ProviderQuery::default())
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?
        .into_iter()
        .filter(|provider| state.check_provider_access(user_id, &provider.name))
        .map(|provider| AggregatedModelListEntry {
            provider_name: provider.name,
        })
        .collect();
    providers.sort_by(|left, right| left.provider_name.cmp(&right.provider_name));
    Ok(providers)
}

fn build_live_model_list_request_body(protocol: ProtocolKind) -> Vec<u8> {
    match protocol {
        ProtocolKind::Claude => serde_json::to_vec(&serde_json::json!({
            "query": { "limit": 1000 }
        }))
        .unwrap_or_default(),
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            serde_json::to_vec(&serde_json::json!({
                "query": { "pageSize": 1000 }
            }))
            .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

async fn execute_live_model_list(
    state: &AppState,
    provider_name: &str,
    protocol: ProtocolKind,
    headers: &http::HeaderMap,
) -> Result<gproxy_sdk::provider::engine::ExecuteResult, HttpError> {
    state
        .engine()
        .execute(ExecuteRequest {
            provider: provider_name.to_string(),
            operation: OperationFamily::ModelList,
            protocol,
            body: build_live_model_list_request_body(protocol),
            headers: headers.clone(),
            model: None,
            forced_credential_index: None,
        })
        .await
        .map_err(Into::into)
}

fn raw_gemini_model_id(name: &str) -> &str {
    name.strip_prefix("models/").unwrap_or(name)
}

async fn build_openai_unscoped_model_list_body(
    state: &AppState,
    user_id: i64,
    headers: &http::HeaderMap,
    trace_id: i64,
) -> Result<Vec<u8>, HttpError> {
    let providers = collect_unscoped_authorized_models(state, user_id).await?;
    let mut models: HashMap<String, gproxy_sdk::protocol::openai::types::OpenAiModel> =
        HashMap::new();
    let mut success_count = 0usize;
    let mut last_error = None;

    for provider in providers {
        match execute_live_model_list(
            state,
            &provider.provider_name,
            ProtocolKind::OpenAi,
            headers,
        )
        .await
        {
            Ok(result) => {
                record_upstream_log(
                    state,
                    trace_id,
                    &provider.provider_name,
                    result.meta.as_ref(),
                )
                .await;
                if !(200..=299).contains(&result.status) {
                    last_error = Some(HttpError::internal(format!(
                        "provider '{}' model list failed with HTTP {}",
                        provider.provider_name, result.status
                    )));
                    continue;
                }
                let ExecuteBody::Full(body) = result.body else {
                    continue;
                };
                let Ok(response) = serde_json::from_slice::<
                    gproxy_sdk::protocol::openai::types::OpenAiModelList,
                >(&body) else {
                    last_error = Some(HttpError::internal(format!(
                        "provider '{}' returned invalid OpenAI model list body",
                        provider.provider_name
                    )));
                    continue;
                };
                success_count += 1;
                for mut model in response.data {
                    if !state.check_model_permission(user_id, &provider.provider_name, &model.id) {
                        continue;
                    }
                    model.id = prefixed_model_id(&provider.provider_name, &model.id);
                    model.owned_by = provider.provider_name.clone();
                    models.insert(model.id.clone(), model);
                }
            }
            Err(err) => last_error = Some(err),
        }
    }

    if success_count == 0 && !models.is_empty() {
        success_count = 1;
    }
    if success_count == 0
        && let Some(err) = last_error
    {
        return Err(err);
    }

    let mut data: Vec<_> = models.into_values().collect();
    data.sort_by(|left, right| left.id.cmp(&right.id));
    let body = gproxy_sdk::protocol::openai::types::OpenAiModelList {
        data,
        object: gproxy_sdk::protocol::openai::types::OpenAiListObject::List,
    };
    serde_json::to_vec(&body).map_err(|e| HttpError::internal(e.to_string()))
}

async fn build_claude_unscoped_model_list_body(
    state: &AppState,
    user_id: i64,
    headers: &http::HeaderMap,
    trace_id: i64,
) -> Result<Vec<u8>, HttpError> {
    let providers = collect_unscoped_authorized_models(state, user_id).await?;
    let mut models: HashMap<String, gproxy_sdk::protocol::claude::types::BetaModelInfo> =
        HashMap::new();
    let mut success_count = 0usize;
    let mut last_error = None;

    for provider in providers {
        match execute_live_model_list(
            state,
            &provider.provider_name,
            ProtocolKind::Claude,
            headers,
        )
        .await
        {
            Ok(result) => {
                record_upstream_log(
                    state,
                    trace_id,
                    &provider.provider_name,
                    result.meta.as_ref(),
                )
                .await;
                if !(200..=299).contains(&result.status) {
                    last_error = Some(HttpError::internal(format!(
                        "provider '{}' model list failed with HTTP {}",
                        provider.provider_name, result.status
                    )));
                    continue;
                }
                let ExecuteBody::Full(body) = result.body else {
                    continue;
                };
                let Ok(response) = serde_json::from_slice::<
                    gproxy_sdk::protocol::claude::model_list::response::ResponseBody,
                >(&body) else {
                    last_error = Some(HttpError::internal(format!(
                        "provider '{}' returned invalid Claude model list body",
                        provider.provider_name
                    )));
                    continue;
                };
                success_count += 1;
                for mut model in response.data {
                    if !state.check_model_permission(user_id, &provider.provider_name, &model.id) {
                        continue;
                    }
                    model.id = prefixed_model_id(&provider.provider_name, &model.id);
                    models.insert(model.id.clone(), model);
                }
            }
            Err(err) => last_error = Some(err),
        }
    }

    if success_count == 0 && !models.is_empty() {
        success_count = 1;
    }
    if success_count == 0
        && let Some(err) = last_error
    {
        return Err(err);
    }

    let mut data: Vec<_> = models.into_values().collect();
    data.sort_by(|left, right| left.id.cmp(&right.id));
    let body = gproxy_sdk::protocol::claude::model_list::response::ResponseBody {
        first_id: data
            .first()
            .map(|model| model.id.clone())
            .unwrap_or_default(),
        has_more: false,
        last_id: data
            .last()
            .map(|model| model.id.clone())
            .unwrap_or_default(),
        data,
    };
    serde_json::to_vec(&body).map_err(|e| HttpError::internal(e.to_string()))
}

async fn build_gemini_unscoped_model_list_body(
    state: &AppState,
    user_id: i64,
    headers: &http::HeaderMap,
    trace_id: i64,
) -> Result<Vec<u8>, HttpError> {
    let providers = collect_unscoped_authorized_models(state, user_id).await?;
    let mut models: HashMap<String, gproxy_sdk::protocol::gemini::types::GeminiModelInfo> =
        HashMap::new();
    let mut success_count = 0usize;
    let mut last_error = None;

    for provider in providers {
        match execute_live_model_list(
            state,
            &provider.provider_name,
            ProtocolKind::Gemini,
            headers,
        )
        .await
        {
            Ok(result) => {
                record_upstream_log(
                    state,
                    trace_id,
                    &provider.provider_name,
                    result.meta.as_ref(),
                )
                .await;
                if !(200..=299).contains(&result.status) {
                    last_error = Some(HttpError::internal(format!(
                        "provider '{}' model list failed with HTTP {}",
                        provider.provider_name, result.status
                    )));
                    continue;
                }
                let ExecuteBody::Full(body) = result.body else {
                    continue;
                };
                let Ok(response) = serde_json::from_slice::<
                    gproxy_sdk::protocol::gemini::model_list::response::ResponseBody,
                >(&body) else {
                    last_error = Some(HttpError::internal(format!(
                        "provider '{}' returned invalid Gemini model list body",
                        provider.provider_name
                    )));
                    continue;
                };
                success_count += 1;
                for mut model in response.models {
                    let raw_model_id = raw_gemini_model_id(&model.name).to_string();
                    if !state.check_model_permission(
                        user_id,
                        &provider.provider_name,
                        &raw_model_id,
                    ) {
                        continue;
                    }
                    let prefixed_id = prefixed_model_id(&provider.provider_name, &raw_model_id);
                    model.name = format!("models/{prefixed_id}");
                    model.base_model_id = model
                        .base_model_id
                        .take()
                        .map(|base_model_id| {
                            prefixed_model_id(&provider.provider_name, &base_model_id)
                        })
                        .or_else(|| Some(prefixed_id.clone()));
                    models.insert(model.name.clone(), model);
                }
            }
            Err(err) => last_error = Some(err),
        }
    }

    if success_count == 0 && !models.is_empty() {
        success_count = 1;
    }
    if success_count == 0
        && let Some(err) = last_error
    {
        return Err(err);
    }

    let mut data: Vec<_> = models.into_values().collect();
    data.sort_by(|left, right| left.name.cmp(&right.name));
    let body = gproxy_sdk::protocol::gemini::model_list::response::ResponseBody {
        models: data,
        next_page_token: None,
    };
    serde_json::to_vec(&body).map_err(|e| HttpError::internal(e.to_string()))
}

async fn build_unscoped_model_list_body(
    state: &AppState,
    user_id: i64,
    protocol: ProtocolKind,
    headers: &http::HeaderMap,
    trace_id: i64,
) -> Result<Vec<u8>, HttpError> {
    match protocol {
        ProtocolKind::Claude => {
            build_claude_unscoped_model_list_body(state, user_id, headers, trace_id).await
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            build_gemini_unscoped_model_list_body(state, user_id, headers, trace_id).await
        }
        _ => build_openai_unscoped_model_list_body(state, user_id, headers, trace_id).await,
    }
}

fn resolve_claude_file_access(
    state: &AppState,
    user_id: i64,
    provider_name: &str,
    file_id: &str,
) -> Result<ClaudeFileAccess, HttpError> {
    let record = state
        .find_user_file(user_id, provider_name, file_id)
        .ok_or_else(|| HttpError::not_found("file not found"))?;
    let (resolved_provider_name, forced_credential_index) = state
        .credential_position_for_id(record.credential_id)
        .ok_or_else(|| HttpError::not_found("file not found"))?;
    if resolved_provider_name != provider_name {
        return Err(HttpError::not_found("file not found"));
    }
    let metadata = state
        .find_claude_file(record.provider_id, &record.file_id)
        .map(|file| file.metadata);
    Ok(ClaudeFileAccess {
        record,
        metadata,
        forced_credential_index,
    })
}

fn build_claude_file_list_body(
    state: &AppState,
    user_id: i64,
    provider_name: &str,
    query: Option<&str>,
) -> Vec<u8> {
    let params = parse_claude_file_list_query(query);
    let mut files: Vec<(
        i64,
        String,
        gproxy_sdk::protocol::claude::types::FileMetadata,
    )> = state
        .list_user_files(user_id, provider_name)
        .into_iter()
        .filter_map(|record| {
            state
                .find_claude_file(record.provider_id, &record.file_id)
                .map(|file| {
                    (
                        file.file_created_at_unix_ms,
                        record.file_id.clone(),
                        file.metadata,
                    )
                })
        })
        .collect();

    files.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));

    if let Some(after_id) = params.after_id.as_deref() {
        if let Some(index) = files.iter().position(|(_, file_id, _)| file_id == after_id) {
            files = files.into_iter().skip(index + 1).collect();
        } else {
            files.clear();
        }
    }
    if let Some(before_id) = params.before_id.as_deref() {
        if let Some(index) = files
            .iter()
            .position(|(_, file_id, _)| file_id == before_id)
        {
            files.truncate(index);
        } else {
            files.clear();
        }
    }

    let has_more = files.len() > params.limit;
    let page: Vec<gproxy_sdk::protocol::claude::types::FileMetadata> = files
        .into_iter()
        .take(params.limit)
        .map(|(_, _, metadata)| metadata)
        .collect();
    let body = gproxy_sdk::protocol::claude::file_list::response::ResponseBody {
        first_id: page.first().map(|metadata| metadata.id.clone()),
        has_more: Some(has_more),
        last_id: page.last().map(|metadata| metadata.id.clone()),
        data: page,
    };
    serde_json::to_vec(&body).unwrap_or_else(|_| b"{\"data\":[]}".to_vec())
}

fn plan_file_operation(
    state: &AppState,
    user_id: i64,
    _user_key_id: i64,
    provider_name: &str,
    operation: OperationFamily,
    request_path: &str,
    request_query: Option<&str>,
) -> Result<Option<FileOperationPlan>, HttpError> {
    if !is_file_operation(operation) {
        return Ok(None);
    }

    match operation {
        OperationFamily::FileUpload => {
            if !state.check_file_permission(user_id, provider_name) {
                return Err(HttpError::forbidden(
                    "file API not authorized for this user",
                ));
            }
            Ok(Some(FileOperationPlan::Upstream {
                forced_credential_index: None,
                deleted_file: None,
            }))
        }
        OperationFamily::FileList => {
            if !state.check_file_permission(user_id, provider_name) {
                return Err(HttpError::forbidden(
                    "file API not authorized for this user",
                ));
            }
            if is_claude_file_provider(state, provider_name) {
                Ok(Some(FileOperationPlan::ShortCircuitJson(
                    build_claude_file_list_body(state, user_id, provider_name, request_query),
                )))
            } else {
                Ok(Some(FileOperationPlan::Upstream {
                    forced_credential_index: None,
                    deleted_file: None,
                }))
            }
        }
        OperationFamily::FileGet => {
            if !state.check_file_permission(user_id, provider_name) {
                return Err(HttpError::forbidden(
                    "file API not authorized for this user",
                ));
            }
            let normalized = normalize_routed_api_path(request_path);
            let file_id = extract_file_id_from_request_path(&normalized)
                .ok_or_else(|| HttpError::bad_request("missing file_id in request path"))?;
            let access = resolve_claude_file_access(state, user_id, provider_name, file_id)?;
            if is_claude_file_provider(state, provider_name)
                && let Some(metadata) = access.metadata
            {
                return Ok(Some(FileOperationPlan::ShortCircuitJson(
                    serde_json::to_vec(&metadata)
                        .unwrap_or_else(|_| b"{\"error\":\"encode file metadata\"}".to_vec()),
                )));
            }
            Ok(Some(FileOperationPlan::Upstream {
                forced_credential_index: Some(access.forced_credential_index),
                deleted_file: None,
            }))
        }
        OperationFamily::FileContent => {
            if !state.check_file_permission(user_id, provider_name) {
                return Err(HttpError::forbidden(
                    "file API not authorized for this user",
                ));
            }
            let normalized = normalize_routed_api_path(request_path);
            let file_id = extract_file_id_from_request_path(&normalized)
                .ok_or_else(|| HttpError::bad_request("missing file_id in request path"))?;
            let access = resolve_claude_file_access(state, user_id, provider_name, file_id)?;
            Ok(Some(FileOperationPlan::Upstream {
                forced_credential_index: Some(access.forced_credential_index),
                deleted_file: None,
            }))
        }
        OperationFamily::FileDelete => {
            if !state.check_file_permission(user_id, provider_name) {
                return Err(HttpError::forbidden(
                    "file API not authorized for this user",
                ));
            }
            let normalized = normalize_routed_api_path(request_path);
            let file_id = extract_file_id_from_request_path(&normalized)
                .ok_or_else(|| HttpError::bad_request("missing file_id in request path"))?;
            let access = resolve_claude_file_access(state, user_id, provider_name, file_id)?;
            Ok(Some(FileOperationPlan::Upstream {
                forced_credential_index: Some(access.forced_credential_index),
                deleted_file: Some(access.record),
            }))
        }
        _ => Ok(None),
    }
}

async fn respond_with_local_json(
    ctx: LocalJsonResponseContext<'_>,
    resp_body: Vec<u8>,
) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(resp_body.clone()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let latency_ms = ctx.start.elapsed().as_millis() as u64;
    tracing::info!(
        ctx.trace_id,
        method = %ctx.req_method,
        path = %ctx.req_path,
        status = 200,
        latency_ms,
        local = true,
        "downstream"
    );
    let resp_headers_json = headers_to_json(response.headers());
    record_downstream_log(
        ctx.state,
        ctx.trace_id,
        ctx.user_id,
        ctx.user_key_id,
        ctx.req_method,
        ctx.req_path,
        ctx.req_query,
        ctx.req_headers_json,
        ctx.req_body,
        Some(200),
        &resp_headers_json,
        Some(&resp_body),
    )
    .await;
    response
}

async fn persist_claude_file_side_effects(ctx: ClaudeFileSideEffectsContext<'_>) {
    if !is_claude_file_provider(ctx.state, ctx.provider_name) {
        return;
    }

    match ctx.operation {
        OperationFamily::FileUpload => {
            if !(200..=299).contains(&ctx.result_status) {
                return;
            }
            let Some(body) = ctx.upload_body.as_deref() else {
                return;
            };
            let Ok(metadata) =
                serde_json::from_slice::<gproxy_sdk::protocol::claude::types::FileMetadata>(body)
            else {
                return;
            };
            let Some(provider_id) = ctx.state.provider_id_for_name(ctx.provider_name) else {
                return;
            };
            let Some(credential_id) = ctx
                .state
                .credential_id_for_index(ctx.provider_name, ctx.result_credential_index)
            else {
                return;
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let file_record = gproxy_server::MemoryUserCredentialFile {
                user_id: ctx.user_id,
                user_key_id: ctx.user_key_id,
                provider_id,
                credential_id,
                file_id: metadata.id.clone(),
                active: true,
                created_at_unix_ms: now_ms,
            };
            let _ = ctx
                .state
                .storage()
                .upsert_user_credential_file(gproxy_storage::UserCredentialFileWrite {
                    user_id: ctx.user_id,
                    user_key_id: ctx.user_key_id,
                    provider_id,
                    credential_id,
                    file_id: metadata.id.clone(),
                    active: true,
                    created_at_unix_ms: now_ms,
                    updated_at_unix_ms: now_ms,
                    deleted_at_unix_ms: None,
                })
                .await;
            let _ = ctx
                .state
                .storage()
                .upsert_claude_file(gproxy_storage::ClaudeFileWrite {
                    provider_id,
                    file_id: metadata.id.clone(),
                    file_created_at: metadata.created_at.clone(),
                    filename: metadata.filename.clone(),
                    mime_type: metadata.mime_type.clone(),
                    size_bytes: metadata.size_bytes as i64,
                    downloadable: metadata.downloadable,
                    raw_json: serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_string()),
                    updated_at_unix_ms: now_ms,
                })
                .await;
            ctx.state.upsert_user_file_in_memory(file_record);
            ctx.state
                .upsert_claude_file_in_memory(gproxy_server::MemoryClaudeFile {
                    provider_id,
                    file_id: metadata.id.clone(),
                    file_created_at_unix_ms: parse_claude_timestamp_ms(&metadata.created_at),
                    metadata: metadata.clone(),
                });
        }
        OperationFamily::FileDelete => {
            if !(200..=299).contains(&ctx.result_status) {
                return;
            }
            let Some(file) = ctx.deleted_file else {
                return;
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let _ = ctx
                .state
                .storage()
                .upsert_user_credential_file(gproxy_storage::UserCredentialFileWrite {
                    user_id: file.user_id,
                    user_key_id: file.user_key_id,
                    provider_id: file.provider_id,
                    credential_id: file.credential_id,
                    file_id: file.file_id.clone(),
                    active: false,
                    created_at_unix_ms: file.created_at_unix_ms,
                    updated_at_unix_ms: now_ms,
                    deleted_at_unix_ms: Some(now_ms),
                })
                .await;
            ctx.state
                .deactivate_user_file_in_memory(file.user_id, file.provider_id, &file.file_id);
        }
        _ => {}
    }
}

/// Shared context for usage recording, avoids passing 8+ args.
#[derive(Clone)]
pub(crate) struct UsageRecordContext {
    pub state: Arc<AppState>,
    pub user_id: i64,
    pub user_key_id: i64,
    pub provider_name: String,
    pub credential_index: Option<usize>,
    pub precomputed_cost: Option<f64>,
    pub model: Option<String>,
    pub billing_context: Option<gproxy_sdk::provider::billing::BillingContext>,
    pub operation: OperationFamily,
    pub protocol: ProtocolKind,
    pub downstream_trace_id: Option<i64>,
}

/// Record usage (cost tracking + storage write). Shared by HTTP and WebSocket handlers.
///
/// When an async usage sink is configured (via `AppState::usage_tx`), the usage
/// record is sent through the mpsc channel for batched, non-blocking DB writes.
/// Otherwise falls back to synchronous storage write.
pub(crate) async fn record_usage(ctx: &UsageRecordContext, usage: &Usage) {
    let cost = ctx
        .precomputed_cost
        .or_else(|| {
            let billing_context = ctx.billing_context.as_ref()?;
            ctx.state
                .engine()
                .estimate_billing(&ctx.provider_name, billing_context, usage)
                .map(|billing| billing.total_cost)
        })
        .unwrap_or(0.0);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let provider_id = ctx.state.provider_id_for_name(&ctx.provider_name);
    let credential_id = ctx
        .credential_index
        .and_then(|index| ctx.state.credential_id_for_index(&ctx.provider_name, index));
    let usage_write = gproxy_storage::UsageWrite {
        downstream_trace_id: ctx.downstream_trace_id,
        at_unix_ms: now_ms,
        provider_id,
        credential_id,
        user_id: Some(ctx.user_id),
        user_key_id: Some(ctx.user_key_id),
        operation: ctx.operation.to_string(),
        protocol: ctx.protocol.to_string(),
        model: ctx.model.clone(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_creation_input_tokens_5min: usage.cache_creation_input_tokens_5min,
        cache_creation_input_tokens_1h: usage.cache_creation_input_tokens_1h,
        cost,
    };

    // Send usage to async sink (includes cost for durable quota tracking).
    // Quota is charged only after usage record is successfully queued/persisted,
    // preventing invisible cost accumulation when records are dropped.
    if let Some(tx) = ctx.state.usage_tx() {
        if let Err(err) = tx.try_send(usage_write) {
            tracing::warn!(user_id = ctx.user_id, %err, "usage sink full, record dropped, quota not charged");
            return;
        }
        if cost > 0.0 {
            ctx.state.add_cost_usage(ctx.user_id, cost);
        }
    } else {
        // Fallback: synchronous DB write (legacy path)
        match ctx
            .state
            .storage()
            .record_usage_and_quota_cost(usage_write, cost)
            .await
        {
            Ok(Some((quota, cost_used))) => {
                ctx.state
                    .upsert_user_quota_in_memory(ctx.user_id, quota, cost_used);
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!(user_id = ctx.user_id, cost, error = %err, "failed to persist usage");
            }
        }
    }
}

fn stream_with_usage_tracking(
    ctx: UsageRecordContext,
    mut stream: gproxy_sdk::provider::engine::ExecuteBodyStream,
) -> impl futures_util::Stream<
    Item = Result<bytes::Bytes, gproxy_sdk::provider::response::UpstreamError>,
> + Send {
    let recorder = StreamUsageRecorder::new(ctx.clone());

    try_stream! {
        let mut decoder = UsageChunkDecoder::new(ctx.protocol);

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            for json_chunk in decoder.push_chunk(&chunk) {
                recorder.observe_json_chunk(&json_chunk);
            }
            yield chunk;
        }

        for json_chunk in decoder.finish() {
            recorder.observe_json_chunk(&json_chunk);
        }

        if let Some(usage) = recorder.finish_completed() {
            record_stream_usage(&ctx, usage).await;
        }
    }
}

#[derive(Default)]
struct StreamUsageRecorderState {
    finalized: bool,
    last_usage: Option<Usage>,
    partial_usage: Usage,
    partial_output: String,
}

struct StreamUsageRecorder {
    ctx: UsageRecordContext,
    state: Arc<Mutex<StreamUsageRecorderState>>,
}

impl StreamUsageRecorder {
    fn new(ctx: UsageRecordContext) -> Self {
        Self {
            ctx,
            state: Arc::new(Mutex::new(StreamUsageRecorderState::default())),
        }
    }

    fn observe_json_chunk(&self, json_chunk: &[u8]) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        if state.finalized {
            return;
        }

        if let Some(usage) =
            gproxy_sdk::provider::usage::extract_stream_usage(self.ctx.protocol, json_chunk)
        {
            merge_usage(&mut state.partial_usage, &usage);
            state.last_usage = Some(usage);
        } else if let Some(usage) = extract_partial_stream_usage(self.ctx.protocol, json_chunk) {
            merge_usage(&mut state.partial_usage, &usage);
        }

        if let Some(text) = extract_partial_output_text(self.ctx.protocol, json_chunk) {
            state.partial_output.push_str(&text);
        }
    }

    fn finish_completed(&self) -> Option<Usage> {
        let mut state = self.state.lock().ok()?;
        state.finalized = true;
        state.last_usage.clone()
    }

    fn take_interrupted_usage(&self) -> Option<Usage> {
        let mut state = self.state.lock().ok()?;
        if state.finalized {
            return None;
        }
        state.finalized = true;

        if let Some(usage) = state.last_usage.clone() {
            return Some(usage);
        }

        let has_partial_usage = usage_has_any_value(&state.partial_usage);
        if !has_partial_usage && state.partial_output.is_empty() {
            return None;
        }

        let mut usage = state.partial_usage.clone();
        if let Some(model) = self.ctx.model.as_deref()
            && !state.partial_output.is_empty()
        {
            let estimated = gproxy_sdk::provider::count_tokens::estimate_partial_usage(
                usage.input_tokens,
                &state.partial_output,
                model,
            );
            usage.output_tokens = estimated.output_tokens;
            if usage.input_tokens.is_none() {
                usage.input_tokens = estimated.input_tokens;
            }
        }

        usage_has_any_value(&usage).then_some(usage)
    }
}

impl Drop for StreamUsageRecorder {
    fn drop(&mut self) {
        let Some(usage) = self.take_interrupted_usage() else {
            return;
        };

        let ctx = self.ctx.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                record_stream_usage(&ctx, usage).await;
            });
        }
    }
}

async fn record_stream_usage(ctx: &UsageRecordContext, usage: Usage) {
    let cost = ctx
        .precomputed_cost
        .or_else(|| {
            let billing_context = ctx.billing_context.as_ref()?;
            ctx.state
                .engine()
                .estimate_billing(&ctx.provider_name, billing_context, &usage)
                .map(|billing| billing.total_cost)
        })
        .unwrap_or(0.0);

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let provider_id = ctx.state.provider_id_for_name(&ctx.provider_name);
    let credential_id = ctx
        .credential_index
        .and_then(|index| ctx.state.credential_id_for_index(&ctx.provider_name, index));
    let usage_write = gproxy_storage::UsageWrite {
        downstream_trace_id: ctx.downstream_trace_id,
        at_unix_ms: now_ms,
        provider_id,
        credential_id,
        user_id: Some(ctx.user_id),
        user_key_id: Some(ctx.user_key_id),
        operation: ctx.operation.to_string(),
        protocol: ctx.protocol.to_string(),
        model: ctx.model.clone(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_creation_input_tokens_5min: usage.cache_creation_input_tokens_5min,
        cache_creation_input_tokens_1h: usage.cache_creation_input_tokens_1h,
        cost,
    };
    match ctx
        .state
        .storage()
        .record_usage_and_quota_cost(usage_write, cost)
        .await
    {
        Ok(Some((quota, cost_used))) => {
            ctx.state
                .upsert_user_quota_in_memory(ctx.user_id, quota, cost_used);
        }
        Ok(None) => {}
        Err(err) => {
            tracing::error!(user_id = ctx.user_id, cost, error = %err, "failed to persist streamed usage and quota atomically");
        }
    }
}

enum UsageChunkDecoder {
    Sse(gproxy_sdk::protocol::stream::SseToNdjsonRewriter),
    Ndjson(Vec<u8>),
}

impl UsageChunkDecoder {
    fn new(protocol: ProtocolKind) -> Self {
        match protocol {
            ProtocolKind::Claude
            | ProtocolKind::OpenAi
            | ProtocolKind::OpenAiResponse
            | ProtocolKind::OpenAiChatCompletion
            | ProtocolKind::Gemini => {
                Self::Sse(gproxy_sdk::protocol::stream::SseToNdjsonRewriter::default())
            }
            ProtocolKind::GeminiNDJson => Self::Ndjson(Vec::new()),
        }
    }

    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Self::Sse(rewriter) => split_usage_lines(&rewriter.push_chunk(chunk), &mut out),
            Self::Ndjson(pending) => {
                pending.extend_from_slice(chunk);
                drain_usage_lines(pending, &mut out);
            }
        }
        out
    }

    fn finish(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Self::Sse(rewriter) => split_usage_lines(&rewriter.finish(), &mut out),
            Self::Ndjson(pending) => {
                if !pending.is_empty() {
                    let mut line = std::mem::take(pending);
                    if line.last().copied() == Some(b'\r') {
                        line.pop();
                    }
                    if !line.is_empty() {
                        out.push(line);
                    }
                }
            }
        }
        out
    }
}

use gproxy_sdk::protocol::stream::{
    drain_lines as drain_usage_lines, split_lines as split_usage_lines,
};

fn usage_has_any_value(usage: &Usage) -> bool {
    usage.input_tokens.is_some()
        || usage.output_tokens.is_some()
        || usage.cache_read_input_tokens.is_some()
        || usage.cache_creation_input_tokens.is_some()
        || usage.cache_creation_input_tokens_5min.is_some()
        || usage.cache_creation_input_tokens_1h.is_some()
}

fn merge_usage(dst: &mut Usage, src: &Usage) {
    if src.input_tokens.is_some() {
        dst.input_tokens = src.input_tokens;
    }
    if src.output_tokens.is_some() {
        dst.output_tokens = src.output_tokens;
    }
    if src.cache_read_input_tokens.is_some() {
        dst.cache_read_input_tokens = src.cache_read_input_tokens;
    }
    if src.cache_creation_input_tokens.is_some() {
        dst.cache_creation_input_tokens = src.cache_creation_input_tokens;
    }
    if src.cache_creation_input_tokens_5min.is_some() {
        dst.cache_creation_input_tokens_5min = src.cache_creation_input_tokens_5min;
    }
    if src.cache_creation_input_tokens_1h.is_some() {
        dst.cache_creation_input_tokens_1h = src.cache_creation_input_tokens_1h;
    }
}

fn extract_partial_stream_usage(protocol: ProtocolKind, json_chunk: &[u8]) -> Option<Usage> {
    match protocol {
        ProtocolKind::Claude => {
            use gproxy_sdk::protocol::claude::create_message::stream::ClaudeStreamEvent;

            let event: ClaudeStreamEvent = serde_json::from_slice(json_chunk).ok()?;
            match event {
                ClaudeStreamEvent::MessageStart { message } => Some(Usage {
                    input_tokens: i64::try_from(message.usage.input_tokens).ok(),
                    output_tokens: i64::try_from(message.usage.output_tokens).ok(),
                    cache_read_input_tokens: message.usage.cache_read_input_tokens.try_into().ok(),
                    cache_creation_input_tokens: message
                        .usage
                        .cache_creation_input_tokens
                        .try_into()
                        .ok(),
                    cache_creation_input_tokens_5min: None,
                    cache_creation_input_tokens_1h: None,
                }),
                _ => None,
            }
        }
        ProtocolKind::OpenAiResponse => {
            use gproxy_sdk::protocol::openai::create_response::stream::ResponseStreamEvent;

            let event: ResponseStreamEvent = serde_json::from_slice(json_chunk).ok()?;
            let response = match event {
                ResponseStreamEvent::Created { response, .. }
                | ResponseStreamEvent::Queued { response, .. }
                | ResponseStreamEvent::InProgress { response, .. }
                | ResponseStreamEvent::Completed { response, .. }
                | ResponseStreamEvent::Incomplete { response, .. }
                | ResponseStreamEvent::Failed { response, .. } => response,
                _ => return None,
            };
            let usage = response.usage?;
            Some(Usage {
                input_tokens: i64::try_from(usage.input_tokens).ok(),
                output_tokens: i64::try_from(usage.output_tokens).ok(),
                cache_read_input_tokens: i64::try_from(usage.input_tokens_details.cached_tokens)
                    .ok(),
                cache_creation_input_tokens: None,
                cache_creation_input_tokens_5min: None,
                cache_creation_input_tokens_1h: None,
            })
        }
        ProtocolKind::OpenAi => {
            use gproxy_sdk::protocol::openai::create_image::stream::ImageGenerationStreamEvent;

            let event: ImageGenerationStreamEvent = serde_json::from_slice(json_chunk).ok()?;
            match event {
                ImageGenerationStreamEvent::Completed { usage, .. } => Some(Usage {
                    input_tokens: i64::try_from(usage.input_tokens).ok(),
                    output_tokens: i64::try_from(usage.output_tokens).ok(),
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    cache_creation_input_tokens_5min: None,
                    cache_creation_input_tokens_1h: None,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn extract_partial_output_text(protocol: ProtocolKind, json_chunk: &[u8]) -> Option<String> {
    match protocol {
        ProtocolKind::Claude => {
            use gproxy_sdk::protocol::claude::create_message::stream::{
                BetaRawContentBlockDelta, ClaudeStreamEvent,
            };

            let event: ClaudeStreamEvent = serde_json::from_slice(json_chunk).ok()?;
            match event {
                ClaudeStreamEvent::ContentBlockDelta { delta, .. } => match delta {
                    BetaRawContentBlockDelta::Text { text } => Some(text),
                    BetaRawContentBlockDelta::Thinking { thinking } => Some(thinking),
                    BetaRawContentBlockDelta::InputJson { partial_json } => Some(partial_json),
                    BetaRawContentBlockDelta::Compaction { content } => content,
                    BetaRawContentBlockDelta::Citations { .. }
                    | BetaRawContentBlockDelta::Signature { .. } => None,
                },
                _ => None,
            }
        }
        ProtocolKind::OpenAiChatCompletion => {
            use gproxy_sdk::protocol::openai::create_chat_completions::stream::ChatCompletionChunk;

            let chunk: ChatCompletionChunk = serde_json::from_slice(json_chunk).ok()?;
            let mut parts = Vec::new();
            for choice in chunk.choices {
                let delta = choice.delta;
                if let Some(text) = delta.content
                    && !text.is_empty()
                {
                    parts.push(text);
                }
                if let Some(text) = delta.reasoning_content
                    && !text.is_empty()
                {
                    parts.push(text);
                }
                if let Some(text) = delta.refusal
                    && !text.is_empty()
                {
                    parts.push(text);
                }
                if let Some(function_call) = delta.function_call {
                    if let Some(name) = function_call.name
                        && !name.is_empty()
                    {
                        parts.push(name);
                    }
                    if let Some(arguments) = function_call.arguments
                        && !arguments.is_empty()
                    {
                        parts.push(arguments);
                    }
                }
                if let Some(tool_calls) = delta.tool_calls {
                    for tool_call in tool_calls {
                        if let Some(function) = tool_call.function {
                            if let Some(name) = function.name
                                && !name.is_empty()
                            {
                                parts.push(name);
                            }
                            if let Some(arguments) = function.arguments
                                && !arguments.is_empty()
                            {
                                parts.push(arguments);
                            }
                        }
                    }
                }
            }
            (!parts.is_empty()).then_some(parts.join("\n"))
        }
        ProtocolKind::OpenAiResponse => {
            use gproxy_sdk::protocol::openai::create_response::stream::ResponseStreamEvent;

            let event: ResponseStreamEvent = serde_json::from_slice(json_chunk).ok()?;
            match event {
                ResponseStreamEvent::OutputTextDelta { delta, .. }
                | ResponseStreamEvent::RefusalDelta { delta, .. }
                | ResponseStreamEvent::ReasoningTextDelta { delta, .. }
                | ResponseStreamEvent::ReasoningSummaryTextDelta { delta, .. }
                | ResponseStreamEvent::FunctionCallArgumentsDelta { delta, .. }
                | ResponseStreamEvent::CustomToolCallInputDelta { delta, .. }
                | ResponseStreamEvent::McpCallArgumentsDelta { delta, .. }
                | ResponseStreamEvent::AudioTranscriptDelta { delta, .. }
                | ResponseStreamEvent::CodeInterpreterCallCodeDelta { delta, .. } => {
                    (!delta.is_empty()).then_some(delta)
                }
                _ => None,
            }
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            use gproxy_sdk::protocol::gemini::generate_content::response::ResponseBody;

            let chunk: ResponseBody = serde_json::from_slice(json_chunk).ok()?;
            let mut parts = Vec::new();
            if let Some(candidates) = chunk.candidates {
                for candidate in candidates {
                    if let Some(content) = candidate.content {
                        for part in content.parts {
                            if let Some(text) = part.text
                                && !text.is_empty()
                            {
                                parts.push(text);
                            }
                            if let Some(function_call) = part.function_call {
                                if !function_call.name.is_empty() {
                                    parts.push(function_call.name);
                                }
                                if let Some(args) = function_call.args
                                    && let Ok(json) = serde_json::to_string(&args)
                                    && !json.is_empty()
                                {
                                    parts.push(json);
                                }
                            }
                        }
                    }
                    if let Some(message) = candidate.finish_message
                        && !message.is_empty()
                    {
                        parts.push(message);
                    }
                }
            }
            if let Some(status) = chunk.model_status
                && let Some(message) = status.message
                && !message.is_empty()
            {
                parts.push(message);
            }
            (!parts.is_empty()).then_some(parts.join("\n"))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Logging helpers
// ---------------------------------------------------------------------------

pub(crate) fn generate_trace_id() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

fn headers_to_json(headers: &http::HeaderMap) -> String {
    let map: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or("")))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "[]".to_string())
}

fn buffered_request_body(request: &Request) -> Result<Vec<u8>, HttpError> {
    request
        .extensions()
        .get::<BufferedBodyBytes>()
        .map(|body| body.0.to_vec())
        .ok_or_else(|| HttpError::internal("buffered request body missing"))
}

fn build_execute_body(
    operation: OperationFamily,
    request_path: &str,
    request_query: Option<&str>,
    original_body: Vec<u8>,
) -> Vec<u8> {
    match operation {
        OperationFamily::ModelList => {
            build_model_request_body(operation, request_path, request_query).unwrap_or_default()
        }
        OperationFamily::FileList
        | OperationFamily::FileGet
        | OperationFamily::FileContent
        | OperationFamily::FileDelete => {
            build_file_request_body(operation, request_path, request_query).unwrap_or_default()
        }
        _ => original_body,
    }
}

fn normalize_unscoped_request_body(
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: Vec<u8>,
    target_model: &str,
) -> Vec<u8> {
    let pointers: &[(&str, bool)] = match (operation, protocol) {
        (OperationFamily::CountToken, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson) => &[
            ("/generate_content_request/model", true),
            ("/generateContentRequest/model", true),
        ],
        (
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini | ProtocolKind::GeminiNDJson,
        )
        | (OperationFamily::Embedding, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson)
        | (OperationFamily::ModelGet, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson)
        | (OperationFamily::ModelList, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson) => &[],
        _ => &[("/model", false)],
    };
    if pointers.is_empty() || body.is_empty() {
        return body;
    }

    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };
    for (pointer, gemini_resource) in pointers {
        let Some(slot) = value.pointer_mut(pointer) else {
            continue;
        };
        let Some(raw) = slot.as_str() else { continue };
        let replacement = if *gemini_resource {
            format!("models/{target_model}")
        } else {
            target_model.to_string()
        };
        if raw != replacement {
            *slot = serde_json::Value::String(replacement);
        }
    }

    serde_json::to_vec(&value).unwrap_or(body)
}

pub(crate) fn extract_requested_total_tokens(
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: &[u8],
) -> Option<i64> {
    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    match (operation, protocol) {
        (
            OperationFamily::GenerateContent
            | OperationFamily::StreamGenerateContent
            | OperationFamily::Compact,
            ProtocolKind::Claude,
        ) => json.get("max_tokens").and_then(|value| value.as_i64()),
        (
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ) => json
            .get("max_completion_tokens")
            .and_then(|value| value.as_i64())
            .or_else(|| json.get("max_tokens").and_then(|value| value.as_i64())),
        (
            OperationFamily::GenerateContent
            | OperationFamily::StreamGenerateContent
            | OperationFamily::Compact,
            ProtocolKind::OpenAiResponse,
        )
        | (OperationFamily::CountToken, ProtocolKind::OpenAi) => json
            .get("max_output_tokens")
            .and_then(|value| value.as_i64()),
        (
            OperationFamily::GenerateContent
            | OperationFamily::StreamGenerateContent
            | OperationFamily::CountToken,
            ProtocolKind::Gemini | ProtocolKind::GeminiNDJson,
        ) => json
            .pointer("/generationConfig/maxOutputTokens")
            .and_then(|value| value.as_i64())
            .or_else(|| {
                json.pointer("/generation_config/max_output_tokens")
                    .and_then(|value| value.as_i64())
            })
            .or_else(|| {
                json.pointer("/generateContentRequest/generationConfig/maxOutputTokens")
                    .and_then(|value| value.as_i64())
            })
            .or_else(|| {
                json.pointer("/generate_content_request/generation_config/max_output_tokens")
                    .and_then(|value| value.as_i64())
            }),
        _ => None,
    }
}

fn build_model_request_body(
    operation: OperationFamily,
    _request_path: &str,
    request_query: Option<&str>,
) -> Option<Vec<u8>> {
    let mut root = serde_json::Map::new();

    match operation {
        OperationFamily::ModelList => {
            let mut query = serde_json::Map::new();
            if let Some(raw_query) = request_query {
                for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
                    match key.as_ref() {
                        "after_id" | "before_id" | "pageToken" => {
                            query.insert(
                                key.into_owned(),
                                serde_json::Value::String(value.into_owned()),
                            );
                        }
                        "limit" | "pageSize" => {
                            if let Ok(number) = value.parse::<u64>() {
                                query.insert(
                                    key.into_owned(),
                                    serde_json::Value::Number(number.into()),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !query.is_empty() {
                root.insert("query".to_string(), serde_json::Value::Object(query));
            }
        }
        _ => return None,
    }

    serde_json::to_vec(&serde_json::Value::Object(root)).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::{Extension, Request, State};
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{Json, Router};
    use bytes::Bytes;
    use gproxy_sdk::provider::engine::{GproxyEngine, ProviderConfig};
    use gproxy_server::middleware::classify::{BufferedBodyBytes, Classification};
    use gproxy_server::middleware::request_model::ExtractedModel;
    use gproxy_server::{
        AppStateBuilder, GlobalConfig, MemoryUser, MemoryUserKey, PermissionEntry, RateLimitRule,
    };
    use gproxy_storage::SeaOrmStorage;
    use serde_json::json;
    use tokio::net::TcpListener;

    use super::proxy_unscoped;
    use crate::auth::AuthenticatedUser;

    async fn spawn_mock_openai_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock upstream");
        let addr = listener.local_addr().expect("mock upstream addr");
        let app = Router::new().route(
            "/v1/chat/completions",
            post(|| async move {
                Json(json!({
                    "id": "chatcmpl-test",
                    "object": "chat.completion",
                    "model": "demo",
                    "choices": [
                        {
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": "ok"
                            },
                            "finish_reason": "stop"
                        }
                    ]
                }))
            }),
        );

        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("mock upstream should serve");
        });

        (format!("http://{addr}"), handle)
    }

    async fn build_unscoped_proxy_state(base_url: String) -> Arc<gproxy_server::AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        let engine = GproxyEngine::builder()
            .add_provider_json(ProviderConfig {
                name: "test".to_string(),
                channel: "custom".to_string(),
                settings_json: json!({
                    "base_url": base_url,
                    "auth_scheme": "bearer"
                }),
                credentials: vec![json!({
                    "api_key": "sk-upstream"
                })],
            })
            .expect("custom provider config should be valid")
            .build();
        let state = AppStateBuilder::new()
            .engine(engine)
            .storage(storage)
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                ..GlobalConfig::default()
            })
            .users(vec![MemoryUser {
                id: 1,
                name: "alice".to_string(),
                enabled: true,
                is_admin: false,
                password_hash: "hash".to_string(),
            }])
            .keys(vec![MemoryUserKey {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            }])
            .build();

        state.replace_provider_names(HashMap::from([("test".to_string(), 42)]));
        state.replace_user_permissions(HashMap::from([(
            1,
            vec![PermissionEntry {
                id: 1,
                provider_id: Some(42),
                model_pattern: "*".to_string(),
            }],
        )]));
        state.replace_user_rate_limits(HashMap::from([(
            1,
            vec![RateLimitRule {
                id: 2,
                model_pattern: "*".to_string(),
                rpm: None,
                rpd: None,
                total_tokens: None,
            }],
        )]));
        state.upsert_user_quota_in_memory(1, 1.0, 0.999);

        Arc::new(state)
    }

    #[tokio::test]
    async fn proxy_unscoped_allows_request_when_quota_service_has_remaining_balance() {
        let (base_url, server_handle) = spawn_mock_openai_server().await;
        let state = build_unscoped_proxy_state(base_url).await;
        let body = serde_json::to_vec(&json!({
            "model": "test/demo",
            "messages": [
                {
                    "role": "user",
                    "content": "hello"
                }
            ]
        }))
        .expect("request body should serialize");

        let mut request = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .body(Body::from(body.clone()))
            .expect("request should build");
        request
            .extensions_mut()
            .insert(BufferedBodyBytes(Bytes::from(body.clone())));
        request.extensions_mut().insert(Classification::new(
            gproxy_server::OperationFamily::GenerateContent,
            gproxy_server::ProtocolKind::OpenAiChatCompletion,
        ));
        request
            .extensions_mut()
            .insert(ExtractedModel(Some("test/demo".to_string())));

        let response = proxy_unscoped(
            State(state),
            Extension(AuthenticatedUser(MemoryUserKey {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            })),
            request,
        )
        .await;

        server_handle.abort();

        let response = response.expect("request should not be rejected before upstream call");
        assert_eq!(response.status(), StatusCode::OK);
    }
}

fn build_file_request_body(
    operation: OperationFamily,
    request_path: &str,
    request_query: Option<&str>,
) -> Option<Vec<u8>> {
    let normalized = normalize_routed_api_path(request_path);
    let mut root = serde_json::Map::new();

    match operation {
        OperationFamily::FileList => {
            let mut query = serde_json::Map::new();
            if let Some(raw_query) = request_query {
                for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
                    match key.as_ref() {
                        "after_id" | "before_id" => {
                            query.insert(
                                key.into_owned(),
                                serde_json::Value::String(value.into_owned()),
                            );
                        }
                        "limit" => {
                            if let Ok(limit) = value.parse::<u64>() {
                                query.insert(
                                    "limit".to_string(),
                                    serde_json::Value::Number(limit.into()),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            if !query.is_empty() {
                root.insert("query".to_string(), serde_json::Value::Object(query));
            }
        }
        OperationFamily::FileGet | OperationFamily::FileContent | OperationFamily::FileDelete => {
            let file_id = extract_file_id_from_request_path(&normalized)?;
            root.insert(
                "path".to_string(),
                serde_json::json!({ "file_id": file_id }),
            );
        }
        _ => return None,
    }

    serde_json::to_vec(&serde_json::Value::Object(root)).ok()
}

fn normalize_routed_api_path(path: &str) -> String {
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let start = if matches!(segments.first(), Some(&"v1" | &"v1beta" | &"v1beta1")) {
        1
    } else if matches!(segments.get(1), Some(&"v1" | &"v1beta" | &"v1beta1")) {
        2
    } else {
        0
    };

    if start >= segments.len() {
        "/".to_string()
    } else {
        format!("/{}", segments[start..].join("/"))
    }
}

fn extract_file_id_from_request_path(path: &str) -> Option<&str> {
    let tail = path.strip_prefix("/files/")?;
    if let Some(file_id) = tail.strip_suffix("/content")
        && !file_id.is_empty()
        && !file_id.contains('/')
    {
        return Some(file_id);
    }
    if !tail.is_empty() && !tail.contains('/') {
        return Some(tail);
    }
    None
}

/// Record upstream request/response log to DB.
async fn record_upstream_log(
    state: &AppState,
    trace_id: i64,
    provider_name: &str,
    meta: Option<&UpstreamRequestMeta>,
) {
    let config = state.config();
    if !config.enable_upstream_log {
        return;
    }
    let Some(meta) = meta else {
        return;
    };
    let include_body = config.enable_upstream_log_body;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let provider_id = state.provider_id_for_name(provider_name);
    let credential_id = meta
        .credential_index
        .and_then(|index| state.credential_id_for_index(provider_name, index));
    let _ = state
        .storage()
        .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertUpstreamRequest(
            gproxy_storage::UpstreamRequestWrite {
                downstream_trace_id: Some(trace_id),
                at_unix_ms: now_ms,
                internal: false,
                provider_id,
                credential_id,
                request_method: meta.method.clone(),
                request_headers_json: serde_json::to_string(&meta.request_headers)
                    .unwrap_or_else(|_| "[]".to_string()),
                request_url: Some(meta.url.clone()),
                request_body: if include_body {
                    meta.request_body.clone()
                } else {
                    None
                },
                response_status: meta.response_status.map(|s| s as i32),
                response_headers_json: serde_json::to_string(&meta.response_headers)
                    .unwrap_or_else(|_| "[]".to_string()),
                response_body: None,
            },
        ))
        .await;
}

/// Record downstream request/response log to DB.
#[allow(clippy::too_many_arguments)]
async fn record_downstream_log(
    state: &AppState,
    trace_id: i64,
    user_id: i64,
    user_key_id: i64,
    method: &str,
    path: &str,
    query: Option<&str>,
    req_headers_json: &str,
    req_body: Option<&Vec<u8>>,
    resp_status: Option<i32>,
    resp_headers_json: &str,
    resp_body: Option<&Vec<u8>>,
) {
    let config = state.config();
    if !config.enable_downstream_log {
        return;
    }
    let include_body = config.enable_downstream_log_body;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let _ = state
        .storage()
        .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertDownstreamRequest(
            gproxy_storage::DownstreamRequestWrite {
                trace_id,
                at_unix_ms: now_ms,
                internal: false,
                user_id: Some(user_id),
                user_key_id: Some(user_key_id),
                request_method: method.to_string(),
                request_headers_json: req_headers_json.to_string(),
                request_path: path.to_string(),
                request_query: query.map(String::from),
                request_body: if include_body {
                    req_body.cloned()
                } else {
                    None
                },
                response_status: resp_status,
                response_headers_json: resp_headers_json.to_string(),
                response_body: if include_body {
                    resp_body.cloned()
                } else {
                    None
                },
            },
        ))
        .await;
}
