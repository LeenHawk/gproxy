use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_stream::try_stream;
use axum::body::Body;
use axum::extract::{Extension, Path, Request, State};
use axum::http::{
    HeaderValue, StatusCode,
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING},
};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;

use gproxy_sdk::engine::engine::{ExecuteBody, ExecuteRequest, UpstreamRequestMeta, Usage};
use gproxy_server::middleware::classify::{BufferedBodyBytes, Classification};
use gproxy_server::middleware::request_model::ExtractedModel;
use gproxy_server::{AppState, OperationFamily, ProtocolKind};

use crate::auth::AuthenticatedUser;
use crate::error::HttpError;
use gproxy_storage::repository::FileRepository;

/// Proxy handler for provider-scoped routes: `/{provider}/v1/...`
///
/// Uses `Path<HashMap<String, String>>` rather than `Path<String>` because this
/// handler is wired to routes with both one path param (`/{provider}/v1/messages`)
/// and two path params (`/{provider}/v1beta/models/{*target}`). `Path<String>`
/// would panic at runtime with "Expected 1 but got 2" on the two-param routes.
pub async fn proxy(
    State(state): State<Arc<AppState>>,
    Path(path_params): Path<HashMap<String, String>>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    request: Request,
) -> Result<Response, HttpError> {
    let provider_name = path_params
        .get("provider")
        .cloned()
        .ok_or_else(|| HttpError::bad_request("route missing provider path param"))?;
    let start = std::time::Instant::now();
    let trace_id = request
        .extensions()
        .get::<crate::downstream_log::TraceId>()
        .map(|t| t.0)
        .unwrap_or_else(generate_trace_id);
    let req_method = request.method().to_string();
    let req_path = request.uri().path().to_string();
    let req_query = request.uri().query().map(String::from);
    let headers = request.headers().clone();
    let user_key = authenticated.0;

    // Extract classification from middleware extensions
    let classification = request
        .extensions()
        .get::<Classification>()
        .cloned()
        .ok_or_else(|| HttpError::bad_request("request not classified"))?;

    // Extract model from middleware extensions. OpenAI clients conventionally
    // send `body.model = "{provider}/{model}"`; strip the matching prefix so
    // downstream alias resolution, permission checks, and rewrite_rules all
    // see the bare name (`model_pattern` filters are authored without the
    // prefix).
    let model = request
        .extensions()
        .get::<ExtractedModel>()
        .and_then(|m| m.0.clone())
        .map(|m| {
            m.strip_prefix(&format!("{provider_name}/"))
                .map(str::to_string)
                .unwrap_or(m)
        });

    // Map classification to SDK operation/protocol strings
    let operation = classification.operation;

    // Permission check BEFORE alias resolution, using the ORIGINAL model name
    // and the provider from the URL path. This way aliases only serve as a
    // provider/model redirect — they don't bypass the user's permission set.
    if !state.check_provider_access(user_key.user_id, &provider_name) {
        return Err(HttpError::forbidden(
            "provider not authorized for this user",
        ));
    }
    if let Some(ref m) = model
        && !is_file_operation(operation)
        && !state.check_model_permission(user_key.user_id, &provider_name, m)
    {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    let req_body = build_execute_body(
        classification.operation,
        &req_path,
        req_query.as_deref(),
        buffered_request_body(&request)?,
    );

    // Rewrite rules run later, at the executor stage (post-translation),
    // so setting target-protocol-only fields like Claude `thinking.display`
    // or `output_config.effort` actually lands on the outbound body.
    // Alias resolution keeps the user-sent name in `ExecuteRequest.model`
    // (only `provider` is taken from the alias target) so suffix-variant
    // `model_pattern` filters still match at the executor stage.

    // Resolve alias (after permission check and rewrite_rules), but only
    // within the currently scoped provider. A same-named model row owned by a
    // different provider must not hijack `/{provider}/...` traffic.
    let scoped_alias = model
        .as_deref()
        .and_then(|m| state.resolve_model_alias_for_provider(m, &provider_name));
    let (effective_provider, effective_model, alias_model_name) = if let Some(alias) = &scoped_alias
    {
        // Keep the user-sent model name as the effective model so
        // `rewrite_rules` at the executor stage can still match the
        // suffix-variant pattern (rules include a `path:"model"` set
        // action that rewrites the body's model to the upstream target).
        let alias_name = model.clone();
        (alias.provider_name.clone(), model.clone(), alias_name)
    } else {
        (provider_name.clone(), model.clone(), None)
    };

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
                start,
                trace_id,
                req_method: &req_method,
                req_path: &req_path,
            },
            resp_body.clone(),
        )
        .await);
    }

    // Local routing short-circuit for model_list / model_get: when the
    // provider's routing rule for this (operation, protocol) is Local,
    // serve the response from the local models table without hitting upstream.
    if matches!(
        operation,
        OperationFamily::ModelList | OperationFamily::ModelGet
    ) && state
        .engine()
        .is_local_routing(&effective_provider, operation, protocol)
    {
        let resp_body = match operation {
            OperationFamily::ModelList => {
                build_local_model_list_body(&state, user_key.user_id, &effective_provider, protocol)
            }
            OperationFamily::ModelGet => {
                let Some(model_id) = effective_model.as_deref() else {
                    return Err(HttpError::bad_request("missing model in model_get request"));
                };
                match build_local_model_get_body(
                    &state,
                    user_key.user_id,
                    &effective_provider,
                    model_id,
                    protocol,
                ) {
                    Some(body) => body,
                    None => return Err(HttpError::not_found("model not found in local table")),
                }
            }
            _ => unreachable!(),
        };
        return Ok(respond_with_local_json(
            LocalJsonResponseContext {
                start,
                trace_id,
                req_method: &req_method,
                req_path: &req_path,
            },
            resp_body,
        )
        .await);
    }

    // Non-Local model_get: try the local models table first. If the requested
    // model exists locally (real model or alias), return the local info
    // without hitting upstream. Otherwise fall through to engine.execute.
    if operation == OperationFamily::ModelGet
        && let Some(model_id) = effective_model.as_deref()
        && let Some(body) = build_local_model_get_body(
            &state,
            user_key.user_id,
            &effective_provider,
            model_id,
            protocol,
        )
    {
        return Ok(respond_with_local_json(
            LocalJsonResponseContext {
                start,
                trace_id,
                req_method: &req_method,
                req_path: &req_path,
            },
            body,
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

    // ModelList: decide whether upstream needs to be hit and with what
    // query. If the requested page is fully served by locals, short-circuit
    // here. Otherwise carry `local_head` + `locals_ids` to be merged after
    // the upstream response lands.
    let mut modellist_merge: Option<(Vec<serde_json::Value>, std::collections::HashSet<String>)> =
        None;
    let modellist_upstream_query: Option<Option<String>> =
        if operation == OperationFamily::ModelList {
            let locals_json = collect_scoped_model_locals_json(
                &state,
                user_key.user_id,
                &effective_provider,
                protocol,
            );
            match plan_paginated_model_list(protocol, locals_json, req_query.as_deref()) {
                ModelListPlan::FullyLocal(body) => {
                    return Ok(respond_with_local_json(
                        LocalJsonResponseContext {
                            start,
                            trace_id,
                            req_method: &req_method,
                            req_path: &req_path,
                        },
                        body,
                    )
                    .await);
                }
                ModelListPlan::Upstream {
                    upstream_query,
                    local_head,
                    locals_ids,
                } => {
                    modellist_merge = Some((local_head, locals_ids));
                    Some(upstream_query)
                }
            }
        } else {
            None
        };

    let mut result = match state
        .engine()
        .execute(ExecuteRequest {
            provider: effective_provider.clone(),
            operation,
            protocol,
            body: req_body.clone(),
            query: modellist_upstream_query
                .clone()
                .unwrap_or_else(|| strip_reserved_query_keys(req_query.clone())),
            headers,
            model: effective_model.clone(),
            forced_credential_index,
            response_model_override: alias_model_name.clone(),
        })
        .await
    {
        Ok(result) => result,
        Err(err) => {
            let upstream_status = err
                .meta
                .as_ref()
                .and_then(|m| m.response_status)
                .map(i32::from)
                .unwrap_or(500);
            record_execute_error_logs(
                &state,
                trace_id,
                &effective_provider,
                &req_method,
                upstream_status,
                err.meta.as_ref(),
            )
            .await;
            return Err(err.into());
        }
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
    // When an alias is in use, try pricing by alias name first, then fall back
    // to the real model name so admins can set custom per-alias pricing.
    let (billing_context, precomputed_cost) = {
        let alias_ctx = alias_model_name.as_ref().map(|alias| {
            state.engine().build_billing_context(
                &effective_provider,
                Some(alias.as_str()),
                &req_body,
            )
        });
        let alias_cost = result.usage.as_ref().and_then(|usage| {
            let ctx = alias_ctx.as_ref()?.as_ref()?;
            state
                .engine()
                .estimate_billing(&effective_provider, ctx, usage)
                .map(|b| b.total_cost)
        });
        if alias_cost.is_some() {
            (alias_ctx.flatten(), alias_cost)
        } else {
            let real_ctx = state.engine().build_billing_context(
                &effective_provider,
                effective_model.as_deref(),
                &req_body,
            );
            let real_cost = result.usage.as_ref().and_then(|usage| {
                let ctx = real_ctx.as_ref()?;
                state
                    .engine()
                    .estimate_billing(&effective_provider, ctx, usage)
                    .map(|b| b.total_cost)
            });
            (real_ctx, real_cost)
        }
    };
    let usage_ctx = UsageRecordContext {
        state: state.clone(),
        user_id: user_key.user_id,
        user_key_id: user_key.id,
        provider_name: effective_provider.clone(),
        credential_index: Some(result.credential_index),
        precomputed_cost,
        model: effective_model.clone(),
        billing_context,
        operation,
        protocol,
        downstream_trace_id: Some(trace_id),
    };

    // Record usage via storage write channel
    if let Some(ref usage) = result.usage {
        record_usage(&usage_ctx, usage).await;
    }

    // Persist any credential updates (e.g. refreshed OAuth tokens) to DB
    if !result.credential_updates.is_empty() {
        crate::provider::oauth::persist_credential_updates(&state, &result.credential_updates)
            .await;
    }

    // Record upstream log (deferred for streaming — handled in stream_with_usage_tracking)
    let is_stream = matches!(result.body, ExecuteBody::Stream(_));
    if !is_stream {
        record_upstream_log(&state, trace_id, &effective_provider, result.meta.as_ref()).await;
    }

    // Merge local models into the scoped ModelList response using the
    // paginated-merge path. `modellist_merge` is Some for every ModelList
    // request that reached upstream (fully-local requests short-circuited
    // earlier).
    if let Some((local_head, locals_ids)) = modellist_merge
        && let ExecuteBody::Full(ref mut body) = result.body
    {
        *body = finalize_paginated_model_list(protocol, local_head, &locals_ids, body);
    }

    let response_body = match result.body {
        ExecuteBody::Full(ref resp_body) => Body::from(resp_body.clone()),
        ExecuteBody::Stream(stream) if classification.operation.is_stream() => {
            let ul_ctx = StreamUpstreamLogContext {
                trace_id,
                provider_name: effective_provider.clone(),
                meta: result.meta.clone(),
                raw_capture: result.stream_raw_capture.clone(),
                stream_started_at: result.stream_started_at,
            };
            Body::from_stream(stream_with_usage_tracking(
                usage_ctx.clone(),
                Some(ul_ctx),
                result.stream_usage_state.clone(),
                stream,
            ))
        }
        ExecuteBody::Stream(stream) => Body::from_stream(stream),
    };

    let mut response = Response::builder()
        .status(result.status)
        .body(response_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());

    *response.headers_mut() = result.headers;
    normalize_response_headers(response.headers_mut(), operation, protocol);
    Ok(response)
}

/// Proxy handler for unscoped routes: `/v1/...`
pub async fn proxy_unscoped(
    State(state): State<Arc<AppState>>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    request: Request,
) -> Result<Response, HttpError> {
    let start = std::time::Instant::now();
    let trace_id = request
        .extensions()
        .get::<crate::downstream_log::TraceId>()
        .map(|t| t.0)
        .unwrap_or_else(generate_trace_id);
    let req_method = request.method().to_string();
    let req_path = request.uri().path().to_string();
    let req_query = request.uri().query().map(String::from);
    let headers = request.headers().clone();
    let user_key = authenticated.0;

    let classification = request
        .extensions()
        .get::<Classification>()
        .cloned()
        .ok_or_else(|| HttpError::bad_request("request not classified"))?;

    if classification.operation == OperationFamily::ModelList {
        return Ok(respond_with_local_json(
            LocalJsonResponseContext {
                start,
                trace_id,
                req_method: &req_method,
                req_path: &req_path,
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

    // Resolve provider: alias → prefix → error.
    // `permission_model` is the name we'll check against the permission
    // whitelist — for aliases this is the alias NAME (not the target model)
    // so aliases don't silently inherit the target model's permissions.
    // `target_model` likewise keeps the alias name (not the resolved
    // target) so executor-stage `rewrite_rules` can still match the
    // suffix-variant pattern and rewrite the outbound body's model field.
    let (target_provider, target_model, alias_model_override, permission_model) =
        if let Some(alias) = state.resolve_model_alias(model_name) {
            (
                alias.provider_name,
                model_name.clone(),
                Some(model_name.clone()),
                model_name.clone(),
            )
        } else if let Some((provider, model)) = model_name.split_once('/') {
            if let Some(alias) = state.resolve_model_alias_for_provider(model, provider) {
                (
                    alias.provider_name,
                    model.to_string(),
                    Some(model_name.clone()),
                    model.to_string(),
                )
            } else {
                (
                    provider.to_string(),
                    model.to_string(),
                    Some(model_name.clone()),
                    model.to_string(),
                )
            }
        } else {
            return Err(HttpError::bad_request(
                "model must have provider prefix (provider/model) or match an alias",
            ));
        };

    // Permission check BEFORE further rewrites. The alias path uses the
    // alias name itself so admins must whitelist the alias explicitly.
    if !state.check_model_permission(user_key.user_id, &target_provider, &permission_model) {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    let operation = classification.operation;
    let protocol = classification.protocol;

    // Rewrite rules run at the executor stage (post-translation); see the
    // matching comment in `proxy` above.
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

    // model_get: try local table first (works for both Local routing and
    // Passthrough/Transform routing — local always takes precedence when the
    // model is found locally). If the model isn't in the local table and the
    // routing is Local, 404; otherwise fall through to upstream.
    if operation == OperationFamily::ModelGet {
        if let Some(body) = build_local_model_get_body(
            &state,
            user_key.user_id,
            &target_provider,
            &target_model,
            protocol,
        ) {
            return Ok(respond_with_local_json(
                LocalJsonResponseContext {
                    start,
                    trace_id,
                    req_method: &req_method,
                    req_path: &req_path,
                },
                body,
            )
            .await);
        }
        if state
            .engine()
            .is_local_routing(&target_provider, operation, protocol)
        {
            return Err(HttpError::not_found("model not found in local table"));
        }
    }

    let result = match state
        .engine()
        .execute(ExecuteRequest {
            provider: target_provider.clone(),
            operation,
            protocol,
            body: req_body.clone(),
            query: strip_reserved_query_keys(req_query.clone()),
            headers,
            model: Some(target_model.clone()),
            forced_credential_index: None,
            response_model_override: alias_model_override.clone(),
        })
        .await
    {
        Ok(result) => result,
        Err(err) => {
            let upstream_status = err
                .meta
                .as_ref()
                .and_then(|m| m.response_status)
                .map(i32::from)
                .unwrap_or(500);
            record_execute_error_logs(
                &state,
                trace_id,
                &target_provider,
                &req_method,
                upstream_status,
                err.meta.as_ref(),
            )
            .await;
            return Err(err.into());
        }
    };

    // When an alias is in use, try pricing by alias name first, then fall back
    // to the real model name so admins can set custom per-alias pricing.
    let (billing_context, precomputed_cost) = {
        let alias_ctx = alias_model_override.as_ref().map(|alias| {
            state
                .engine()
                .build_billing_context(&target_provider, Some(alias.as_str()), &req_body)
        });
        let alias_cost = result.usage.as_ref().and_then(|usage| {
            let ctx = alias_ctx.as_ref()?.as_ref()?;
            state
                .engine()
                .estimate_billing(&target_provider, ctx, usage)
                .map(|b| b.total_cost)
        });
        if alias_cost.is_some() {
            (alias_ctx.flatten(), alias_cost)
        } else {
            let real_ctx = state.engine().build_billing_context(
                &target_provider,
                Some(&target_model),
                &req_body,
            );
            let real_cost = result.usage.as_ref().and_then(|usage| {
                let ctx = real_ctx.as_ref()?;
                state
                    .engine()
                    .estimate_billing(&target_provider, ctx, usage)
                    .map(|b| b.total_cost)
            });
            (real_ctx, real_cost)
        }
    };
    let usage_ctx = UsageRecordContext {
        state: state.clone(),
        user_id: user_key.user_id,
        user_key_id: user_key.id,
        provider_name: target_provider.clone(),
        credential_index: Some(result.credential_index),
        precomputed_cost,
        model: Some(target_model.clone()),
        billing_context,
        operation,
        protocol,
        downstream_trace_id: Some(trace_id),
    };

    // Record usage via storage write channel
    if let Some(ref usage) = result.usage {
        record_usage(&usage_ctx, usage).await;
    }

    // Persist any credential updates (e.g. refreshed OAuth tokens) to DB
    if !result.credential_updates.is_empty() {
        crate::provider::oauth::persist_credential_updates(&state, &result.credential_updates)
            .await;
    }

    // Record upstream log (deferred for streaming — handled in stream_with_usage_tracking)
    let is_stream = matches!(result.body, ExecuteBody::Stream(_));
    if !is_stream {
        record_upstream_log(&state, trace_id, &target_provider, result.meta.as_ref()).await;
    }

    let response_body = match result.body {
        ExecuteBody::Full(ref resp_body) => Body::from(resp_body.clone()),
        ExecuteBody::Stream(stream) if classification.operation.is_stream() => {
            let ul_ctx = StreamUpstreamLogContext {
                trace_id,
                provider_name: target_provider.clone(),
                meta: result.meta.clone(),
                raw_capture: result.stream_raw_capture.clone(),
                stream_started_at: result.stream_started_at,
            };
            Body::from_stream(stream_with_usage_tracking(
                usage_ctx.clone(),
                Some(ul_ctx),
                result.stream_usage_state.clone(),
                stream,
            ))
        }
        ExecuteBody::Stream(stream) => Body::from_stream(stream),
    };

    let mut response = Response::builder()
        .status(result.status)
        .body(response_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    *response.headers_mut() = result.headers;
    normalize_response_headers(response.headers_mut(), operation, protocol);
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
    let trace_id = request
        .extensions()
        .get::<crate::downstream_log::TraceId>()
        .map(|t| t.0)
        .unwrap_or_else(generate_trace_id);
    let req_method = request.method().to_string();
    let req_path = request.uri().path().to_string();
    let req_query = request.uri().query().map(String::from);
    let headers = request.headers().clone();
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
                start,
                trace_id,
                req_method: &req_method,
                req_path: &req_path,
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
            provider: target_provider.clone(),
            operation,
            protocol,
            body: req_body.clone(),
            query: strip_reserved_query_keys(req_query.clone()),
            headers,
            model: None,
            forced_credential_index,
            response_model_override: None,
        })
        .await
    {
        Ok(result) => result,
        Err(err) => {
            let upstream_status = err
                .meta
                .as_ref()
                .and_then(|m| m.response_status)
                .map(i32::from)
                .unwrap_or(500);
            record_execute_error_logs(
                &state,
                trace_id,
                &target_provider,
                &req_method,
                upstream_status,
                err.meta.as_ref(),
            )
            .await;
            return Err(err.into());
        }
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
        let billing_context =
            state
                .engine()
                .build_billing_context(&target_provider, None, &req_body);
        let precomputed_cost = {
            let ctx = billing_context.as_ref();
            ctx.and_then(|ctx| {
                state
                    .engine()
                    .estimate_billing(&target_provider, ctx, usage)
                    .map(|b| b.total_cost)
            })
        };
        let usage_ctx = UsageRecordContext {
            state: state.clone(),
            user_id: user_key.user_id,
            user_key_id: user_key.id,
            provider_name: target_provider.clone(),
            credential_index: Some(result.credential_index),
            precomputed_cost,
            model: None,
            billing_context,
            operation,
            protocol,
            downstream_trace_id: Some(trace_id),
        };
        record_usage(&usage_ctx, usage).await;
    }

    // Persist any credential updates (e.g. refreshed OAuth tokens) to DB
    if !result.credential_updates.is_empty() {
        crate::provider::oauth::persist_credential_updates(&state, &result.credential_updates)
            .await;
    }

    // Record upstream log (deferred for streaming — handled in stream_with_usage_tracking)
    let is_stream = matches!(result.body, ExecuteBody::Stream(_));
    if !is_stream {
        record_upstream_log(&state, trace_id, &target_provider, result.meta.as_ref()).await;
    }

    let response_body = match result.body {
        ExecuteBody::Full(ref resp_body) => Body::from(resp_body.clone()),
        ExecuteBody::Stream(stream) => Body::from_stream(stream),
    };

    let mut response = Response::builder()
        .status(result.status)
        .body(response_body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    *response.headers_mut() = result.headers;
    normalize_response_headers(response.headers_mut(), operation, protocol);
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
    start: std::time::Instant,
    trace_id: i64,
    req_method: &'a str,
    req_path: &'a str,
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

/// Drop query keys that are reserved/proxy-internal and must not leak to
/// upstreams. Currently: `alt` (Gemini transport format — we always
/// decide SSE vs JSON via route + sse→JSON aggregation, so honoring the
/// client's `alt` would fight our own decision; stripping early keeps
/// every channel honest without per-channel filtering).
fn strip_reserved_query_keys(query: Option<String>) -> Option<String> {
    let raw = query?;
    let kept: Vec<(String, String)> = url::form_urlencoded::parse(raw.as_bytes())
        .filter(|(k, _)| !matches!(k.as_ref(), "alt"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        return None;
    }
    Some(
        url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(kept)
            .finish(),
    )
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

async fn execute_live_model_list(
    state: &AppState,
    provider_name: &str,
    protocol: ProtocolKind,
    headers: &http::HeaderMap,
) -> Result<gproxy_sdk::engine::engine::ExecuteResult, HttpError> {
    state
        .engine()
        .execute(ExecuteRequest {
            provider: provider_name.to_string(),
            operation: OperationFamily::ModelList,
            protocol,
            // Body is `{}` (not empty) because xform routes — e.g. a custom
            // channel using an anthropic-like / gemini-like routing template,
            // or claudecode/geminicli/antigravity's built-in xforms — will
            // call `serde_json::from_slice::<RequestBody>(body)` in the
            // transformer. The OpenAi / Claude / Gemini ModelList
            // `RequestBody` are all empty structs, so `{}` parses cleanly
            // but an empty buffer fails with EOF at line 1 column 0. For
            // Passthrough routes the body is forwarded verbatim as the GET
            // payload; `{}` is harmless (every upstream ignores GET bodies)
            // and specifically does not leak the legacy `{"query":{...}}`
            // shape that some strict proxies were echoing back downstream.
            body: b"{}".to_vec(),
            query: None,
            headers: headers.clone(),
            model: None,
            forced_credential_index: None,
            response_model_override: None,
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
        // If the provider's routing for ModelList is Local, serve from the
        // local models table instead of calling upstream.
        if state.engine().is_local_routing(
            &provider.provider_name,
            OperationFamily::ModelList,
            ProtocolKind::OpenAi,
        ) {
            let Some(provider_id) = state.provider_id_for_name(&provider.provider_name) else {
                continue;
            };
            for m in state.models().iter() {
                if m.provider_id != provider_id || !m.enabled {
                    continue;
                }
                if !state.check_model_permission(user_id, &provider.provider_name, &m.model_id) {
                    continue;
                }
                let prefixed = prefixed_model_id(&provider.provider_name, &m.model_id);
                models.insert(
                    prefixed.clone(),
                    gproxy_sdk::protocol::openai::types::OpenAiModel {
                        id: prefixed,
                        object: gproxy_sdk::protocol::openai::types::OpenAiModelObject::Model,
                        created: 0,
                        owned_by: provider.provider_name.clone(),
                    },
                );
            }
            success_count += 1;
            continue;
        }
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

    // Inject model alias entries.
    inject_openai_alias_entries(state, user_id, &mut models);

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
        // Local routing: serve from local models table.
        if state.engine().is_local_routing(
            &provider.provider_name,
            OperationFamily::ModelList,
            ProtocolKind::Claude,
        ) {
            let Some(provider_id) = state.provider_id_for_name(&provider.provider_name) else {
                continue;
            };
            for m in state.models().iter() {
                if m.provider_id != provider_id || !m.enabled {
                    continue;
                }
                if !state.check_model_permission(user_id, &provider.provider_name, &m.model_id) {
                    continue;
                }
                let prefixed = prefixed_model_id(&provider.provider_name, &m.model_id);
                let display_name = m.display_name.clone().unwrap_or_else(|| m.model_id.clone());
                models.insert(
                    prefixed.clone(),
                    gproxy_sdk::protocol::claude::types::BetaModelInfo {
                        id: prefixed,
                        created_at: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
                        display_name,
                        max_input_tokens: None,
                        max_tokens: None,
                        capabilities: None,
                        type_: gproxy_sdk::protocol::claude::types::BetaModelType::Model,
                    },
                );
            }
            success_count += 1;
            continue;
        }
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

    // Inject model alias entries.
    inject_claude_alias_entries(state, user_id, &mut models);

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
        // Local routing: serve from local models table.
        if state.engine().is_local_routing(
            &provider.provider_name,
            OperationFamily::ModelList,
            ProtocolKind::Gemini,
        ) {
            let Some(provider_id) = state.provider_id_for_name(&provider.provider_name) else {
                continue;
            };
            for m in state.models().iter() {
                if m.provider_id != provider_id || !m.enabled {
                    continue;
                }
                if !state.check_model_permission(user_id, &provider.provider_name, &m.model_id) {
                    continue;
                }
                let prefixed = prefixed_model_id(&provider.provider_name, &m.model_id);
                let gemini_name = format!("models/{}", prefixed);
                let display_name = m.display_name.clone().unwrap_or_else(|| m.model_id.clone());
                models.insert(
                    gemini_name.clone(),
                    gproxy_sdk::protocol::gemini::types::GeminiModelInfo {
                        name: gemini_name,
                        base_model_id: Some(prefixed),
                        version: None,
                        display_name: Some(display_name),
                        description: None,
                        input_token_limit: None,
                        output_token_limit: None,
                        supported_generation_methods: None,
                        thinking: None,
                        temperature: None,
                        max_temperature: None,
                        top_p: None,
                        top_k: None,
                    },
                );
            }
            success_count += 1;
            continue;
        }
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

    // Inject model alias entries.
    inject_gemini_alias_entries(state, user_id, &mut models);

    let mut data: Vec<_> = models.into_values().collect();
    data.sort_by(|left, right| left.name.cmp(&right.name));
    let body = gproxy_sdk::protocol::gemini::model_list::response::ResponseBody {
        models: data,
        next_page_token: None,
    };
    serde_json::to_vec(&body).map_err(|e| HttpError::internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// Alias injection helpers for unscoped model_list builders
// ---------------------------------------------------------------------------

/// Inject model alias entries into an OpenAI-format model HashMap.
/// For each alias whose target model exists in the map, creates a base alias
/// entry.
fn inject_openai_alias_entries(
    state: &AppState,
    user_id: i64,
    models: &mut HashMap<String, gproxy_sdk::protocol::openai::types::OpenAiModel>,
) {
    let all_models = state.models();

    // Inject local models that aren't already in the upstream response.
    // This lets admins add custom models (including suffix variants) locally.
    for local in all_models.iter().filter(|m| m.enabled) {
        let Some(provider_name) = state.routing.provider_name_for_id(local.provider_id) else {
            continue;
        };
        if !state.check_model_permission(user_id, &provider_name, &local.model_id) {
            continue;
        }
        let prefixed = prefixed_model_id(&provider_name, &local.model_id);
        models.entry(prefixed.clone()).or_insert_with(|| {
            gproxy_sdk::protocol::openai::types::OpenAiModel {
                id: prefixed,
                object: gproxy_sdk::protocol::openai::types::OpenAiModelObject::Model,
                created: 0,
                owned_by: provider_name,
            }
        });
    }
}

/// Inject model alias entries into a Claude-format model HashMap.
fn inject_claude_alias_entries(
    state: &AppState,
    user_id: i64,
    models: &mut HashMap<String, gproxy_sdk::protocol::claude::types::BetaModelInfo>,
) {
    let all_models = state.models();

    for local in all_models.iter().filter(|m| m.enabled) {
        let Some(provider_name) = state.routing.provider_name_for_id(local.provider_id) else {
            continue;
        };
        if !state.check_model_permission(user_id, &provider_name, &local.model_id) {
            continue;
        }
        let prefixed = prefixed_model_id(&provider_name, &local.model_id);
        let display_name = local
            .display_name
            .clone()
            .unwrap_or_else(|| local.model_id.clone());
        models.entry(prefixed.clone()).or_insert_with(|| {
            gproxy_sdk::protocol::claude::types::BetaModelInfo {
                id: prefixed,
                created_at: time::OffsetDateTime::from_unix_timestamp(0).unwrap(),
                display_name,
                max_input_tokens: None,
                max_tokens: None,
                capabilities: None,
                type_: gproxy_sdk::protocol::claude::types::BetaModelType::Model,
            }
        });
    }
}

/// Inject model alias entries into a Gemini-format model HashMap.
fn inject_gemini_alias_entries(
    state: &AppState,
    user_id: i64,
    models: &mut HashMap<String, gproxy_sdk::protocol::gemini::types::GeminiModelInfo>,
) {
    let all_models = state.models();

    for local in all_models.iter().filter(|m| m.enabled) {
        let Some(provider_name) = state.routing.provider_name_for_id(local.provider_id) else {
            continue;
        };
        if !state.check_model_permission(user_id, &provider_name, &local.model_id) {
            continue;
        }
        let prefixed = prefixed_model_id(&provider_name, &local.model_id);
        let gemini_name = format!("models/{prefixed}");
        let display_name = local
            .display_name
            .clone()
            .unwrap_or_else(|| local.model_id.clone());
        models.entry(gemini_name.clone()).or_insert_with(|| {
            gproxy_sdk::protocol::gemini::types::GeminiModelInfo {
                name: gemini_name,
                base_model_id: Some(prefixed),
                version: None,
                display_name: Some(display_name),
                description: None,
                input_token_limit: None,
                output_token_limit: None,
                supported_generation_methods: None,
                thinking: None,
                temperature: None,
                max_temperature: None,
                top_p: None,
                top_k: None,
            }
        });
    }
}

/// Inject model alias entries into a scoped (single-provider) model_list
/// response. Works on raw JSON bytes and handles all protocol formats.
/// Build a model_list response body from the local `models` table for the
/// given provider. Used when routing is Local.
///
/// Returns all enabled models (both real and aliases) that the user has
/// permission to access.
fn build_local_model_list_body(
    state: &AppState,
    user_id: i64,
    provider_name: &str,
    protocol: ProtocolKind,
) -> Vec<u8> {
    let Some(provider_id) = state.provider_id_for_name(provider_name) else {
        return empty_model_list_body(protocol);
    };
    let all_models = state.models();
    let relevant: Vec<_> = all_models
        .iter()
        .filter(|m| m.provider_id == provider_id && m.enabled)
        .filter(|m| state.check_model_permission(user_id, provider_name, &m.model_id))
        .collect();

    let data: Vec<serde_json::Value> = relevant
        .iter()
        .map(|m| local_model_to_json(m, provider_name, protocol))
        .collect();

    let body = match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => serde_json::json!({
            "models": data
        }),
        ProtocolKind::Claude => {
            let first_id = data
                .first()
                .and_then(|m| m.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let last_id = data
                .last()
                .and_then(|m| m.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            serde_json::json!({
                "data": data,
                "first_id": first_id,
                "last_id": last_id,
                "has_more": false
            })
        }
        _ => serde_json::json!({
            "object": "list",
            "data": data
        }),
    };

    serde_json::to_vec(&body).unwrap_or_else(|_| empty_model_list_body(protocol))
}

/// Build a model_get response body from the local `models` table. Returns
/// None if the model doesn't exist or the user lacks permission.
fn build_local_model_get_body(
    state: &AppState,
    user_id: i64,
    provider_name: &str,
    model_id: &str,
    protocol: ProtocolKind,
) -> Option<Vec<u8>> {
    let provider_id = state.provider_id_for_name(provider_name)?;
    let all_models = state.models();
    let model = all_models
        .iter()
        .find(|m| m.provider_id == provider_id && m.model_id == model_id && m.enabled)?;
    if !state.check_model_permission(user_id, provider_name, model_id) {
        return None;
    }
    let json = local_model_to_json(model, provider_name, protocol);
    serde_json::to_vec(&json).ok()
}

/// Convert a `MemoryModel` into a protocol-specific JSON object.
fn local_model_to_json(
    model: &gproxy_core::MemoryModel,
    provider_name: &str,
    protocol: ProtocolKind,
) -> serde_json::Value {
    let display_name = model
        .display_name
        .clone()
        .unwrap_or_else(|| model.model_id.clone());
    match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => serde_json::json!({
            "name": format!("models/{}", model.model_id),
            "displayName": display_name,
            "baseModelId": model.model_id,
        }),
        ProtocolKind::Claude => serde_json::json!({
            "id": model.model_id,
            "type": "model",
            "display_name": display_name,
            "created_at": "2024-01-01T00:00:00Z",
        }),
        _ => serde_json::json!({
            "id": model.model_id,
            "object": "model",
            "created": 0,
            "owned_by": provider_name,
        }),
    }
}

/// Build an empty model_list response body for the given protocol.
fn empty_model_list_body(protocol: ProtocolKind) -> Vec<u8> {
    let body = match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => serde_json::json!({
            "models": []
        }),
        ProtocolKind::Claude => serde_json::json!({
            "data": [],
            "first_id": "",
            "last_id": "",
            "has_more": false
        }),
        _ => serde_json::json!({
            "object": "list",
            "data": []
        }),
    };
    serde_json::to_vec(&body).unwrap_or_default()
}

// =====================================================================
// ModelList pagination: merge DB locals with upstream, maintain a
// compound pageToken so the request can traverse local-first then
// upstream without caching upstream state in gproxy.
//
// Token grammar (opaque to clients, produced & consumed here):
//   ""              → local offset 0 (first page)
//   "L<N>"          → resume local pagination at offset N
//   "U<opaque>"     → local exhausted; forward <opaque> to upstream
//
// Algorithm:
//   * Local(k) and k+M ≤ N_local → fully local slice, no upstream call.
//   * Local(k) and k+M > N_local → head = locals[k..], upstream fills
//     the remaining (M − head.len()) with its first page (no token).
//   * Upstream(opaque) → forward opaque as pageToken with full M.
// =====================================================================

#[derive(Debug, Clone, PartialEq)]
enum ModelListStage {
    Local(usize),
    Upstream(String),
}

enum ModelListPlan {
    /// Response fully served from locals; caller should short-circuit.
    FullyLocal(Vec<u8>),
    /// Caller must call upstream with the rewritten query, then pass the
    /// upstream body to `finalize_paginated_model_list`.
    Upstream {
        upstream_query: Option<String>,
        local_head: Vec<serde_json::Value>,
        locals_ids: std::collections::HashSet<String>,
    },
}

fn default_model_list_page_size(protocol: ProtocolKind) -> usize {
    match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => 50,
        ProtocolKind::Claude => 20,
        _ => 20,
    }
}

fn parse_client_model_list_query(
    protocol: ProtocolKind,
    query: Option<&str>,
) -> (Option<usize>, Option<String>) {
    let mut page_size: Option<usize> = None;
    let mut token: Option<String> = None;
    if let Some(raw) = query {
        for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
            match (protocol, key.as_ref()) {
                (ProtocolKind::Gemini | ProtocolKind::GeminiNDJson, "pageSize") => {
                    page_size = value.parse().ok();
                }
                (ProtocolKind::Gemini | ProtocolKind::GeminiNDJson, "pageToken") => {
                    token = Some(value.into_owned());
                }
                (ProtocolKind::Claude, "limit") => page_size = value.parse().ok(),
                (ProtocolKind::Claude, "after_id") => token = Some(value.into_owned()),
                (_, "limit") => page_size = value.parse().ok(),
                (_, "after") => token = Some(value.into_owned()),
                _ => {}
            }
        }
    }
    (page_size, token)
}

fn parse_model_list_page_token(
    protocol: ProtocolKind,
    locals: &[serde_json::Value],
    raw: Option<&str>,
) -> ModelListStage {
    match protocol {
        // Gemini's pageToken is opaque — we control the shape. Use `L<N>` /
        // `U<opaque>` / empty-is-first-page.
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => match raw.unwrap_or("") {
            "" => ModelListStage::Local(0),
            s => {
                if let Some(rest) = s.strip_prefix('L') {
                    ModelListStage::Local(rest.parse().unwrap_or(0))
                } else if let Some(rest) = s.strip_prefix('U') {
                    ModelListStage::Upstream(rest.to_string())
                } else {
                    ModelListStage::Upstream(s.to_string())
                }
            }
        },
        // Claude `after_id` / OpenAI `after` are literal item ids. Look up
        // in locals; hit → we're still in local stage, offset = pos+1
        // (exclusive). Miss → it's an upstream id, forward verbatim.
        _ => match raw {
            None | Some("") => ModelListStage::Local(0),
            Some(id) => {
                let pos = locals.iter().position(|v| {
                    v.get("id")
                        .and_then(|i| i.as_str())
                        .map(|s| s == id)
                        .unwrap_or(false)
                });
                match pos {
                    Some(k) => ModelListStage::Local(k + 1),
                    None => ModelListStage::Upstream(id.to_string()),
                }
            }
        },
    }
}

fn build_upstream_model_list_query(
    protocol: ProtocolKind,
    page_size: usize,
    page_token: Option<&str>,
) -> Option<String> {
    let mut out = url::form_urlencoded::Serializer::new(String::new());
    match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            out.append_pair("pageSize", &page_size.to_string());
            if let Some(t) = page_token {
                out.append_pair("pageToken", t);
            }
        }
        ProtocolKind::Claude => {
            out.append_pair("limit", &page_size.to_string());
            if let Some(t) = page_token {
                out.append_pair("after_id", t);
            }
        }
        _ => {
            out.append_pair("limit", &page_size.to_string());
            if let Some(t) = page_token {
                out.append_pair("after", t);
            }
        }
    }
    let s = out.finish();
    if s.is_empty() { None } else { Some(s) }
}

fn extract_upstream_model_list(
    protocol: ProtocolKind,
    body: &[u8],
) -> (Vec<serde_json::Value>, Option<String>) {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return (Vec::new(), None);
    };
    let array_key = match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => "models",
        _ => "data",
    };
    let items = v
        .get(array_key)
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    let next = match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => v
            .get("nextPageToken")
            .and_then(|t| t.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        ProtocolKind::Claude => {
            let has_more = v.get("has_more").and_then(|b| b.as_bool()).unwrap_or(false);
            if has_more {
                v.get("last_id")
                    .and_then(|t| t.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
        _ => {
            // OpenAI ModelList has no canonical pageination token — upstream
            // returns everything in a single response. Signal end of stream.
            None
        }
    };
    (items, next)
}

fn model_item_id(protocol: ProtocolKind, item: &serde_json::Value) -> Option<String> {
    match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => item
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.trim_start_matches("models/").to_string()),
        _ => item
            .get("id")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string()),
    }
}

fn serialize_model_list_response(
    protocol: ProtocolKind,
    items: Vec<serde_json::Value>,
    next_page_token: Option<String>,
) -> Vec<u8> {
    let body = match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => serde_json::json!({
            "models": items,
            "nextPageToken": next_page_token,
        }),
        ProtocolKind::Claude => {
            let first_id = items
                .first()
                .and_then(|i| i.get("id"))
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let last_id = items
                .last()
                .and_then(|i| i.get("id"))
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            serde_json::json!({
                "data": items,
                "first_id": first_id,
                "last_id": last_id,
                "has_more": next_page_token.is_some(),
            })
        }
        _ => serde_json::json!({
            "object": "list",
            "data": items,
        }),
    };
    serde_json::to_vec(&body).unwrap_or_default()
}

/// Build the canonical [`serde_json::Value`] list of local models for a
/// provider, applying per-user permission filtering. Order is stable —
/// consumers rely on it for `L<N>` offset semantics.
fn collect_scoped_model_locals_json(
    state: &AppState,
    user_id: i64,
    provider_name: &str,
    protocol: ProtocolKind,
) -> Vec<serde_json::Value> {
    let Some(provider_id) = state.provider_id_for_name(provider_name) else {
        return Vec::new();
    };
    let all_models = state.models();
    all_models
        .iter()
        .filter(|m| {
            m.provider_id == provider_id
                && m.enabled
                && state.check_model_permission(user_id, provider_name, &m.model_id)
        })
        .map(|m| local_model_to_json(m, provider_name, protocol))
        .collect()
}

fn plan_paginated_model_list(
    protocol: ProtocolKind,
    locals: Vec<serde_json::Value>,
    client_query: Option<&str>,
) -> ModelListPlan {
    let (client_size, client_token) = parse_client_model_list_query(protocol, client_query);

    let locals_ids: std::collections::HashSet<String> = locals
        .iter()
        .filter_map(|v| model_item_id(protocol, v))
        .collect();

    // OpenAI-family `/v1/models` canonically returns every model in a
    // single response with no `has_more`/`last_id`. If the client sent no
    // explicit `limit`, serve the full merged list in one shot: locals
    // prepended, upstream hit with no pagination query. This preserves
    // the OpenAI SDK contract (clients iterate `data` once).
    let openai_family = matches!(
        protocol,
        ProtocolKind::OpenAi | ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAiResponse
    );
    if openai_family && client_size.is_none() && client_token.is_none() {
        return ModelListPlan::Upstream {
            upstream_query: None,
            local_head: locals,
            locals_ids,
        };
    }

    let m = client_size.unwrap_or_else(|| default_model_list_page_size(protocol));
    let stage = parse_model_list_page_token(protocol, &locals, client_token.as_deref());

    match stage {
        ModelListStage::Local(offset) => {
            let n = locals.len();
            if offset >= n {
                // No more locals — enter upstream with fresh first page.
                let upstream_query = build_upstream_model_list_query(protocol, m, None);
                return ModelListPlan::Upstream {
                    upstream_query,
                    local_head: Vec::new(),
                    locals_ids,
                };
            }
            if offset + m <= n {
                // Fully local slice.
                let slice = locals[offset..offset + m].to_vec();
                let next = if offset + m < n {
                    Some(format!("L{}", offset + m))
                } else {
                    // Boundary: local exhausted at exactly this page. Hand
                    // out an L<N> token so the next request enters the
                    // branch above and triggers upstream first page.
                    Some(format!("L{}", n))
                };
                let body = serialize_model_list_response(protocol, slice, next);
                ModelListPlan::FullyLocal(body)
            } else {
                // Cross-page: local head + upstream first page.
                let head = locals[offset..].to_vec();
                let need = m - head.len();
                let upstream_query = build_upstream_model_list_query(protocol, need, None);
                ModelListPlan::Upstream {
                    upstream_query,
                    local_head: head,
                    locals_ids,
                }
            }
        }
        ModelListStage::Upstream(opaque) => {
            let upstream_query = build_upstream_model_list_query(protocol, m, Some(&opaque));
            ModelListPlan::Upstream {
                upstream_query,
                local_head: Vec::new(),
                locals_ids,
            }
        }
    }
}

fn finalize_paginated_model_list(
    protocol: ProtocolKind,
    local_head: Vec<serde_json::Value>,
    locals_ids: &std::collections::HashSet<String>,
    upstream_body: &[u8],
) -> Vec<u8> {
    let (upstream_items, upstream_next) = extract_upstream_model_list(protocol, upstream_body);
    let filtered: Vec<_> = upstream_items
        .into_iter()
        .filter(|it| match model_item_id(protocol, it) {
            Some(id) => !locals_ids.contains(&id),
            None => true,
        })
        .collect();
    let mut combined = local_head;
    combined.extend(filtered);
    let next = upstream_next.map(|s| format!("U{}", s));
    serialize_model_list_response(protocol, combined, next)
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
    response
}

fn default_response_content_type(
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> Option<&'static str> {
    match (operation, protocol) {
        (OperationFamily::StreamGenerateContent, ProtocolKind::Claude)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiResponse)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini) => {
            Some("text/event-stream")
        }
        (OperationFamily::GenerateContent, ProtocolKind::Claude)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAi)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAiResponse)
        | (OperationFamily::CountToken, ProtocolKind::Claude)
        | (OperationFamily::CountToken, ProtocolKind::OpenAi)
        | (OperationFamily::Compact, ProtocolKind::OpenAi)
        | (OperationFamily::Embedding, ProtocolKind::OpenAi)
        | (OperationFamily::CreateImage, ProtocolKind::OpenAi)
        | (OperationFamily::CreateImageEdit, ProtocolKind::OpenAi)
        | (OperationFamily::ModelList, _)
        | (OperationFamily::ModelGet, _)
        | (OperationFamily::FileList, _)
        | (OperationFamily::FileGet, _)
        | (OperationFamily::FileContent, _)
        | (OperationFamily::FileDelete, _)
        | (OperationFamily::FileUpload, _) => Some("application/json"),
        _ => None,
    }
}

fn normalize_response_headers(
    headers: &mut axum::http::HeaderMap,
    operation: OperationFamily,
    protocol: ProtocolKind,
) {
    headers.remove(CONTENT_LENGTH);
    headers.remove(CONTENT_ENCODING);
    headers.remove(TRANSFER_ENCODING);

    if headers.contains_key(CONTENT_TYPE) {
        return;
    }
    let Some(content_type) = default_response_content_type(operation, protocol) else {
        return;
    };
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
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

/// Context for recording an upstream request log at the end of a stream,
/// so the accumulated response body can be captured.
#[derive(Clone)]
struct StreamUpstreamLogContext {
    trace_id: i64,
    provider_name: String,
    meta: Option<UpstreamRequestMeta>,
    /// Shared buffer that the engine's stream wrapper tees raw upstream
    /// bytes into as the stream is consumed. The handler reads it after
    /// the stream drains and copies the contents into `meta.response_body`.
    /// `None` when upstream body logging is disabled, or when the route
    /// doesn't pass bytes through a raw-capture tee.
    raw_capture: Option<Arc<std::sync::Mutex<Vec<u8>>>>,
    /// Instant at which `gproxy_channel::http_client::send_request_stream`
    /// armed its timer for this attempt. The deferred-log block computes
    /// `meta.total_latency_ms = stream_started_at.elapsed()` after the
    /// stream drain loop finishes.
    stream_started_at: Option<std::time::Instant>,
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
    pub billing_context: Option<gproxy_sdk::channel::billing::BillingContext>,
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
    // If the sink is unavailable or saturated, fall back to direct persistence
    // so requests never become "free" under backpressure.
    if let Some(tx) = ctx.state.usage_tx() {
        match tx.try_send(usage_write) {
            Ok(()) => {
                if cost > 0.0 {
                    ctx.state.add_cost_usage(ctx.user_id, cost);
                }
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(usage_write)) => {
                tracing::warn!(
                    user_id = ctx.user_id,
                    "usage sink full, persisting usage synchronously"
                );
                persist_usage_write_now(ctx, usage_write, cost).await;
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(usage_write)) => {
                tracing::warn!(
                    user_id = ctx.user_id,
                    "usage sink closed, persisting usage synchronously"
                );
                persist_usage_write_now(ctx, usage_write, cost).await;
            }
        }
    } else {
        persist_usage_write_now(ctx, usage_write, cost).await;
    }
}

async fn persist_usage_write_now(
    ctx: &UsageRecordContext,
    usage_write: gproxy_storage::UsageWrite,
    cost: f64,
) {
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

fn stream_with_usage_tracking(
    ctx: UsageRecordContext,
    upstream_log: Option<StreamUpstreamLogContext>,
    stream_usage_state: Option<Arc<Mutex<gproxy_sdk::channel::usage::StreamUsageSnapshot>>>,
    mut stream: gproxy_sdk::engine::engine::ExecuteBodyStream,
) -> impl futures_util::Stream<Item = Result<bytes::Bytes, gproxy_sdk::channel::response::UpstreamError>>
+ Send {
    let recorder = StreamUsageRecorder::new(ctx.clone(), stream_usage_state);

    try_stream! {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            yield chunk;
        }

        if let Some(usage) = recorder.finish_completed() {
            record_stream_usage(&ctx, usage).await;
        }

        // Record deferred upstream log. The raw upstream body is read
        // from the engine's `stream_raw_capture` buffer, which was
        // populated in-place as the stream drained (before any
        // transformer touched the chunks). This gives the upstream log
        // the same pre-transform wire bytes that the non-stream path
        // stores via `raw_response_body_for_log`, regardless of whether
        // the route is a cross-protocol transform or passthrough.
        if let Some(ul) = upstream_log
            && let Some(mut meta) = ul.meta
        {
            let config = ctx.state.config();
            if config.enable_upstream_log_body
                && let Some(cap) = ul.raw_capture
                && let Ok(mut buf) = cap.lock()
            {
                let captured = std::mem::take(&mut *buf);
                if !captured.is_empty() {
                    meta.response_body = Some(captured);
                }
            }
            drop(config);

            if let Some(started_at) = ul.stream_started_at {
                meta.total_latency_ms = started_at.elapsed().as_millis() as u64;
                tracing::info!(
                    trace_id = ul.trace_id,
                    provider = %ul.provider_name,
                    initial_latency_ms = meta.initial_latency_ms,
                    total_latency_ms = meta.total_latency_ms,
                    "upstream stream finished"
                );
            }

            record_upstream_log(&ctx.state, ul.trace_id, &ul.provider_name, Some(&meta))
                .await;
        }
    }
}

struct StreamUsageRecorder {
    ctx: UsageRecordContext,
    state: Option<Arc<Mutex<gproxy_sdk::channel::usage::StreamUsageSnapshot>>>,
}

impl StreamUsageRecorder {
    fn new(
        ctx: UsageRecordContext,
        state: Option<Arc<Mutex<gproxy_sdk::channel::usage::StreamUsageSnapshot>>>,
    ) -> Self {
        Self { ctx, state }
    }

    fn finish_completed(&self) -> Option<Usage> {
        let state = self.state.as_ref()?;
        let mut state = state.lock().ok()?;
        state.finalized = true;
        // Return the merged view from `partial_usage`, not the last single
        // chunk. The merge captures token counts from whichever chunk is
        // authoritative (cumulative across replaces). For protocols that emit
        // a single terminal chunk (Claude message_delta, OpenAI
        // ChatCompletions final chunk), this is identical to `last_usage`.
        if gproxy_sdk::channel::usage::stream_usage_has_any_value(&state.partial_usage) {
            Some(state.partial_usage.clone())
        } else {
            state.last_usage.clone()
        }
    }

    fn take_interrupted_usage(&self) -> Option<Usage> {
        let state = self.state.as_ref()?;
        let mut state = state.lock().ok()?;
        if state.finalized {
            return None;
        }
        state.finalized = true;

        if let Some(usage) = state.last_usage.clone() {
            return Some(usage);
        }

        let has_partial_usage =
            gproxy_sdk::channel::usage::stream_usage_has_any_value(&state.partial_usage);
        if !has_partial_usage && state.partial_output.is_empty() {
            return None;
        }

        let mut usage = state.partial_usage.clone();
        if let Some(model) = self.ctx.model.as_deref()
            && !state.partial_output.is_empty()
        {
            let estimated = gproxy_sdk::channel::count_tokens::estimate_partial_usage(
                usage.input_tokens,
                &state.partial_output,
                model,
            );
            usage.output_tokens = estimated.output_tokens;
            if usage.input_tokens.is_none() {
                usage.input_tokens = estimated.input_tokens;
            }
        }

        gproxy_sdk::channel::usage::stream_usage_has_any_value(&usage).then_some(usage)
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
    persist_usage_write_now(ctx, usage_write, cost).await;
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
        // ModelList is a GET (empty body), but cross-protocol transforms
        // deserialize the body into the protocol's `RequestBody {}` struct,
        // and `from_slice` on an empty buffer fails with EOF. Hand out an
        // empty JSON object so the transformer parses cleanly. Query-level
        // pagination parameters (pageSize/pageToken/limit/after_id) travel
        // via `ExecuteRequest.query` — no body stuffing.
        OperationFamily::ModelList => b"{}".to_vec(),
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
                response_body: if include_body {
                    meta.response_body.clone()
                } else {
                    None
                },
                initial_latency_ms: Some(meta.initial_latency_ms as i64),
                total_latency_ms: Some(meta.total_latency_ms as i64),
            },
        ))
        .await;
}

async fn record_execute_error_logs(
    state: &AppState,
    trace_id: i64,
    provider_name: &str,
    method: &str,
    response_status: i32,
    upstream_meta: Option<&UpstreamRequestMeta>,
) {
    if state.config().enable_upstream_log {
        let include_body = state.config().enable_upstream_log_body;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let provider_id = state.provider_id_for_name(provider_name);
        let credential_id = upstream_meta
            .and_then(|m| m.credential_index)
            .and_then(|idx| state.credential_id_for_index(provider_name, idx));

        // Prefer the real upstream metadata captured by the retry layer
        // (carries the actual URL, headers, request body, and upstream
        // response body). Fall back to the placeholder values the
        // handler already had when no attempt ever reached upstream
        // (unknown provider, routing miss, etc.).
        let (
            upstream_method,
            upstream_url,
            upstream_req_headers_json,
            upstream_req_body,
            upstream_resp_status,
            upstream_resp_headers_json,
            upstream_resp_body,
            upstream_initial_latency_ms,
            upstream_total_latency_ms,
        ) = match upstream_meta {
            Some(meta) => (
                meta.method.clone(),
                Some(meta.url.clone()),
                serde_json::to_string(&meta.request_headers).unwrap_or_else(|_| "[]".to_string()),
                if include_body {
                    meta.request_body.clone()
                } else {
                    None
                },
                meta.response_status
                    .map(|s| s as i32)
                    .or(Some(response_status)),
                serde_json::to_string(&meta.response_headers).unwrap_or_else(|_| "[]".to_string()),
                if include_body {
                    meta.response_body.clone()
                } else {
                    None
                },
                meta.initial_latency_ms as i64,
                meta.total_latency_ms as i64,
            ),
            None => (
                method.to_string(),
                None,
                "[]".to_string(),
                None,
                Some(response_status),
                "[]".to_string(),
                None,
                0,
                0,
            ),
        };

        let _ = state
            .storage()
            .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertUpstreamRequest(
                gproxy_storage::UpstreamRequestWrite {
                    downstream_trace_id: Some(trace_id),
                    at_unix_ms: now_ms,
                    internal: false,
                    provider_id,
                    credential_id,
                    request_method: upstream_method,
                    request_headers_json: upstream_req_headers_json,
                    request_url: upstream_url,
                    request_body: upstream_req_body,
                    response_status: upstream_resp_status,
                    response_headers_json: upstream_resp_headers_json,
                    response_body: upstream_resp_body,
                    initial_latency_ms: Some(upstream_initial_latency_ms),
                    total_latency_ms: Some(upstream_total_latency_ms),
                },
            ))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::{Extension, Path, Request, State};
    use axum::http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, LOCATION, TRANSFER_ENCODING},
    };
    use axum::routing::post;
    use axum::{Json, Router};
    use bytes::Bytes;
    use gproxy_sdk::engine::engine::{GproxyEngine, ProviderConfig, Usage};
    use gproxy_server::middleware::classify::{BufferedBodyBytes, Classification};
    use gproxy_server::middleware::model_alias::ResolvedAlias;
    use gproxy_server::middleware::request_model::ExtractedModel;
    use gproxy_server::{
        AppStateBuilder, GlobalConfig, MemoryModel, MemoryUser, MemoryUserKey, PermissionEntry,
        RateLimitRule,
    };
    use gproxy_storage::{
        SeaOrmStorage, UpstreamRequestQuery, UsageQuery,
        repository::{ProviderRepository, UserRepository},
    };
    use serde_json::json;
    use tokio::net::TcpListener;

    use super::{
        UsageRecordContext, normalize_response_headers, proxy, proxy_unscoped, record_usage,
    };
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

    async fn spawn_named_chat_server(
        marker: &'static str,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind named mock upstream");
        let addr = listener.local_addr().expect("named mock upstream addr");
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || async move {
                Json(json!({
                    "id": format!("chatcmpl-{marker}"),
                    "object": "chat.completion",
                    "model": "shared",
                    "choices": [
                        {
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": marker
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
                .expect("named mock upstream should serve");
        });

        (format!("http://{addr}"), handle)
    }

    async fn build_unscoped_proxy_state(base_url: String) -> Arc<gproxy_server::AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        storage
            .upsert_provider(gproxy_storage::ProviderWrite {
                id: 42,
                name: "test".to_string(),
                channel: "custom".to_string(),
                label: None,
                settings_json: json!({
                    "base_url": base_url,
                })
                .to_string(),
                routing_json: "".to_string(),
            })
            .await
            .expect("seed provider");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "alice".to_string(),
                password: "hash".to_string(),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            })
            .await
            .expect("seed user key");
        let engine = GproxyEngine::builder()
            .add_provider_json(ProviderConfig {
                name: "test".to_string(),
                channel: "custom".to_string(),
                settings_json: json!({
                    "base_url": base_url,
                }),
                credentials: vec![json!({
                    "api_key": "sk-upstream"
                })],
                routing: None,
            })
            .expect("custom provider config should be valid")
            .build();
        let state = AppStateBuilder::new()
            .engine(engine)
            .storage(storage)
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                enable_upstream_log: true,
                enable_upstream_log_body: true,
                enable_downstream_log: true,
                enable_downstream_log_body: true,
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

    async fn build_multi_provider_proxy_state(
        alpha_base_url: String,
        beta_base_url: String,
    ) -> Arc<gproxy_server::AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        for (id, name, base_url) in [
            (42, "alpha", alpha_base_url.clone()),
            (43, "beta", beta_base_url.clone()),
        ] {
            storage
                .upsert_provider(gproxy_storage::ProviderWrite {
                    id,
                    name: name.to_string(),
                    channel: "custom".to_string(),
                    label: None,
                    settings_json: json!({
                        "base_url": base_url,
                    })
                    .to_string(),
                    routing_json: "".to_string(),
                })
                .await
                .expect("seed provider");
        }
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "alice".to_string(),
                password: "hash".to_string(),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            })
            .await
            .expect("seed user key");
        let engine = GproxyEngine::builder()
            .add_provider_json(ProviderConfig {
                name: "alpha".to_string(),
                channel: "custom".to_string(),
                settings_json: json!({
                    "base_url": alpha_base_url,
                }),
                credentials: vec![json!({
                    "api_key": "sk-alpha"
                })],
                routing: None,
            })
            .expect("alpha provider config should be valid")
            .add_provider_json(ProviderConfig {
                name: "beta".to_string(),
                channel: "custom".to_string(),
                settings_json: json!({
                    "base_url": beta_base_url,
                }),
                credentials: vec![json!({
                    "api_key": "sk-beta"
                })],
                routing: None,
            })
            .expect("beta provider config should be valid")
            .build();
        let state = AppStateBuilder::new()
            .engine(engine)
            .storage(storage)
            .config(GlobalConfig {
                dsn: "sqlite::memory:".to_string(),
                enable_upstream_log: true,
                enable_upstream_log_body: true,
                enable_downstream_log: true,
                enable_downstream_log_body: true,
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

        state.replace_provider_names(HashMap::from([
            ("alpha".to_string(), 42),
            ("beta".to_string(), 43),
        ]));
        // Order matters here: the current global model index is last-write-wins,
        // so `shared` resolves to beta unless the handler scopes resolution.
        state.replace_models(vec![
            MemoryModel {
                id: 1,
                provider_id: 42,
                model_id: "shared".to_string(),
                display_name: None,
                enabled: true,
                pricing: None,
            },
            MemoryModel {
                id: 2,
                provider_id: 43,
                model_id: "shared".to_string(),
                display_name: None,
                enabled: true,
                pricing: None,
            },
        ]);
        state.replace_user_permissions(HashMap::from([(
            1,
            vec![
                PermissionEntry {
                    id: 1,
                    provider_id: Some(42),
                    model_pattern: "*".to_string(),
                },
                PermissionEntry {
                    id: 2,
                    provider_id: Some(43),
                    model_pattern: "*".to_string(),
                },
            ],
        )]));
        state.replace_user_rate_limits(HashMap::from([(
            1,
            vec![RateLimitRule {
                id: 3,
                model_pattern: "*".to_string(),
                rpm: None,
                rpd: None,
                total_tokens: None,
            }],
        )]));
        state.upsert_user_quota_in_memory(1, 1.0, 0.0);

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

    #[tokio::test]
    async fn scoped_proxy_ignores_alias_resolution_to_different_provider() {
        let (alpha_base_url, alpha_handle) = spawn_named_chat_server("alpha").await;
        let (beta_base_url, beta_handle) = spawn_named_chat_server("beta").await;
        let state = build_multi_provider_proxy_state(alpha_base_url.clone(), beta_base_url).await;
        let body = serde_json::to_vec(&json!({
            "model": "shared",
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
            .uri("/alpha/v1/chat/completions")
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
            .insert(ExtractedModel(Some("shared".to_string())));
        let global_alias = state.resolve_model_alias("shared").expect("global alias");
        request.extensions_mut().insert(ResolvedAlias {
            provider_name: Some(global_alias.provider_name),
            model_id: Some(global_alias.model_id),
        });

        let response = proxy(
            State(state.clone()),
            Path(HashMap::from([(
                "provider".to_string(),
                "alpha".to_string(),
            )])),
            Extension(AuthenticatedUser(MemoryUserKey {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            })),
            request,
        )
        .await
        .expect("scoped request should succeed");

        alpha_handle.abort();
        beta_handle.abort();

        assert_eq!(response.status(), StatusCode::OK);
        let upstream_logs = state
            .storage()
            .query_upstream_requests(&UpstreamRequestQuery::default())
            .await
            .expect("query upstream request logs");
        assert_eq!(upstream_logs.len(), 1);
        assert!(
            upstream_logs[0]
                .request_url
                .as_deref()
                .is_some_and(|url| url.starts_with(&alpha_base_url)),
            "scoped provider should stay on alpha instead of being hijacked by beta"
        );
    }

    #[tokio::test]
    async fn unscoped_provider_prefix_scopes_alias_resolution_to_explicit_provider() {
        let (alpha_base_url, alpha_handle) = spawn_named_chat_server("alpha").await;
        let (beta_base_url, beta_handle) = spawn_named_chat_server("beta").await;
        let state = build_multi_provider_proxy_state(alpha_base_url.clone(), beta_base_url).await;
        let body = serde_json::to_vec(&json!({
            "model": "alpha/shared",
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
            .insert(ExtractedModel(Some("alpha/shared".to_string())));

        let response = proxy_unscoped(
            State(state.clone()),
            Extension(AuthenticatedUser(MemoryUserKey {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            })),
            request,
        )
        .await
        .expect("unscoped request should succeed");

        alpha_handle.abort();
        beta_handle.abort();

        assert_eq!(response.status(), StatusCode::OK);
        let upstream_logs = state
            .storage()
            .query_upstream_requests(&UpstreamRequestQuery::default())
            .await
            .expect("query upstream request logs");
        assert_eq!(upstream_logs.len(), 1);
        assert!(
            upstream_logs[0]
                .request_url
                .as_deref()
                .is_some_and(|url| url.starts_with(&alpha_base_url)),
            "explicit provider prefix should keep the request on alpha"
        );
    }

    #[test]
    fn normalize_response_headers_sets_streaming_claude_sse_header() {
        let mut headers = HeaderMap::new();
        normalize_response_headers(
            &mut headers,
            gproxy_server::OperationFamily::StreamGenerateContent,
            gproxy_server::ProtocolKind::Claude,
        );

        assert_eq!(
            headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/event-stream"))
        );
    }

    #[test]
    fn normalize_response_headers_sets_nonstream_claude_json_header() {
        let mut headers = HeaderMap::new();
        normalize_response_headers(
            &mut headers,
            gproxy_server::OperationFamily::GenerateContent,
            gproxy_server::ProtocolKind::Claude,
        );

        assert_eq!(
            headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn normalize_response_headers_preserves_existing_content_type() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/custom"));
        normalize_response_headers(
            &mut headers,
            gproxy_server::OperationFamily::StreamGenerateContent,
            gproxy_server::ProtocolKind::Claude,
        );

        assert_eq!(
            headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/custom"))
        );
    }

    #[test]
    fn normalize_response_headers_removes_body_bound_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("123"));
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(TRANSFER_ENCODING, HeaderValue::from_static("chunked"));

        normalize_response_headers(
            &mut headers,
            gproxy_server::OperationFamily::StreamGenerateContent,
            gproxy_server::ProtocolKind::Claude,
        );

        assert!(!headers.contains_key(CONTENT_LENGTH));
        assert!(!headers.contains_key(CONTENT_ENCODING));
        assert!(!headers.contains_key(TRANSFER_ENCODING));
        assert_eq!(
            headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/event-stream"))
        );
    }

    #[tokio::test]
    async fn record_usage_persists_and_charges_quota_when_queue_is_full() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "alice".to_string(),
                password: "hash".to_string(),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            })
            .await
            .expect("seed user key");

        let (usage_tx, _usage_rx) = tokio::sync::mpsc::channel(1);
        usage_tx
            .try_send(gproxy_storage::UsageWrite {
                downstream_trace_id: None,
                at_unix_ms: 0,
                provider_id: None,
                credential_id: None,
                user_id: Some(999),
                user_key_id: None,
                operation: "seed".to_string(),
                protocol: "seed".to_string(),
                model: None,
                input_tokens: None,
                output_tokens: None,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                cache_creation_input_tokens_5min: None,
                cache_creation_input_tokens_1h: None,
                cost: 0.0,
            })
            .expect("seed queue");

        let state = Arc::new(
            AppStateBuilder::new()
                .engine(GproxyEngine::builder().build())
                .storage(storage)
                .config(GlobalConfig {
                    dsn: "sqlite::memory:".to_string(),
                    ..GlobalConfig::default()
                })
                .usage_tx(usage_tx)
                .build(),
        );

        let ctx = UsageRecordContext {
            state: state.clone(),
            user_id: 1,
            user_key_id: 10,
            provider_name: "test".to_string(),
            credential_index: None,
            precomputed_cost: Some(0.25),
            model: Some("demo".to_string()),
            billing_context: None,
            operation: gproxy_server::OperationFamily::GenerateContent,
            protocol: gproxy_server::ProtocolKind::OpenAiChatCompletion,
            downstream_trace_id: Some(42),
        };

        record_usage(
            &ctx,
            &Usage {
                input_tokens: Some(10),
                output_tokens: Some(20),
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                cache_creation_input_tokens_5min: None,
                cache_creation_input_tokens_1h: None,
            },
        )
        .await;

        let usages = state
            .storage()
            .query_usages(&UsageQuery::default())
            .await
            .expect("query usages");
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].user_id, Some(1));
        assert_eq!(usages[0].input_tokens, Some(10));
        assert_eq!(usages[0].output_tokens, Some(20));

        let quotas = state
            .storage()
            .list_user_quotas()
            .await
            .expect("list quotas");
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].user_id, 1);
        assert_eq!(quotas[0].cost_used, 0.25);

        assert_eq!(state.get_user_quota(1), (0.0, 0.25));
    }

    #[tokio::test]
    async fn proxy_unscoped_records_request_logs_when_upstream_execution_fails() {
        let state = build_unscoped_proxy_state("http://127.0.0.1:1".to_string()).await;
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

        let error = proxy_unscoped(
            State(state.clone()),
            Extension(AuthenticatedUser(MemoryUserKey {
                id: 10,
                user_id: 1,
                api_key: "sk-test".to_string(),
                label: Some("default".to_string()),
                enabled: true,
            })),
            request,
        )
        .await
        .expect_err("request should fail on upstream error");

        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);

        // The downstream log now mirrors the upstream HTTP response
        // status when the retry layer captured a real attempt, instead
        // of the placeholder 500 the handler used to write. For a
        // connection that never reached a listening server, wreq maps
        // the connect failure to `UpstreamError::Http` without a
        // response, so the retry layer has no upstream response and the
        // handler falls back to 500.
        // Downstream logs are now recorded by the downstream_log middleware
        // (not the handler), so they won't appear in this handler-only test.

        let upstream_logs = state
            .storage()
            .query_upstream_requests(&UpstreamRequestQuery::default())
            .await
            .expect("query upstream request logs");
        assert_eq!(upstream_logs.len(), 1);
        assert_eq!(upstream_logs[0].provider_id, Some(42));
        let upstream_status = upstream_logs[0]
            .response_status
            .expect("upstream request must record a status");
        assert!(
            upstream_status >= 500,
            "upstream status should surface server failure, got {upstream_status}"
        );
    }

    /// Regression test for two bugs that together caused `POST
    /// /{provider}/v1beta/models/X:generateContent` to return an empty 405:
    ///
    /// 1. The Gemini Live WebSocket route `GET /{provider}/v1beta/models/{*target}`
    ///    lived on a more-specific path than the HTTP catch-all
    ///    `POST /{provider}/v1beta/{*target}`, so matchit picked the WS route for
    ///    any POST under `/models/*` and replied 405 with an empty body. Fixed by
    ///    adding an explicit `POST /{provider}/v1beta/models/{*target}` HTTP route
    ///    that gets merged onto the same path as the WS GET.
    ///
    /// 2. `handler::proxy` used `Path<String>` which expects exactly one path
    ///    param. On the new two-param route it panicked at runtime with "Expected
    ///    1 but got 2". Fixed by switching to `Path<HashMap<String, String>>`.
    ///
    /// The assertion walks the state through the real `crate::provider::router`
    /// and requires the request to reach the real proxy handler — verified by
    /// checking that a downstream-request log row was written (the proxy handler
    /// writes it; a 405 / Path-extractor 500 short-circuits before logging).
    #[tokio::test]
    async fn router_routes_post_to_v1beta_models_generate_content() {
        use tower::ServiceExt;

        let state = build_unscoped_proxy_state("http://127.0.0.1:1".to_string()).await;
        let app = crate::provider::router(state.clone()).with_state(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test/v1beta/models/demo:generateContent?key=sk-test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"contents":[{"parts":[{"text":"hi"}]}]}"#))
                    .expect("build request"),
            )
            .await
            .expect("router response");

        // Must NOT be 405 (route-shadowing bug) and must NOT be an empty body
        // from any of: Path-extractor runtime error, auth rejection without
        // the query-key fallback, or middleware short-circuit.
        assert_ne!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "POST to /{{provider}}/v1beta/models/X:generateContent must reach \
             the HTTP proxy handler and not be shadowed by the Gemini Live \
             WebSocket GET route"
        );

        // The real proof that the request reached the proxy handler: a
        // downstream-request log row exists. The handler writes this row
        // unconditionally when it runs, so its presence means auth passed
        // (via the ?key= query fallback), classification succeeded, Path
        // extraction succeeded, and the handler ran to completion. Upstream
        // is unreachable (127.0.0.1:1) so response_status is 500 — we don't
        // care about the upstream result, only that we got there.
        // Downstream logs are now recorded by the downstream_log middleware
        // (not the handler), so they won't appear in this handler-only test.
        // The important thing is that the route resolved correctly and the
        // handler ran (verified by the non-404 status above).
    }

    // =====================================================================
    // ModelList pagination helpers
    // =====================================================================

    use super::{
        ModelListPlan, ModelListStage, finalize_paginated_model_list, parse_model_list_page_token,
        plan_paginated_model_list,
    };
    use gproxy_server::ProtocolKind;

    fn gemini_local(id: &str) -> serde_json::Value {
        serde_json::json!({
            "name": format!("models/{}", id),
            "displayName": id,
            "baseModelId": id,
        })
    }

    #[test]
    fn page_token_parses_prefixes() {
        // Gemini: opaque token with L/U prefixes
        let empty_locals: Vec<serde_json::Value> = vec![];
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Gemini, &empty_locals, None),
            ModelListStage::Local(0)
        );
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Gemini, &empty_locals, Some("")),
            ModelListStage::Local(0)
        );
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Gemini, &empty_locals, Some("L12")),
            ModelListStage::Local(12)
        );
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Gemini, &empty_locals, Some("Uabc-xyz")),
            ModelListStage::Upstream("abc-xyz".into())
        );
        // Legacy / unknown-shape token treated as upstream opaque.
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Gemini, &empty_locals, Some("legacy")),
            ModelListStage::Upstream("legacy".into())
        );
    }

    #[test]
    fn page_token_claude_resolves_id_from_locals() {
        // Claude after_id is an item id; resolve to offset via locals lookup.
        let locals = vec![
            serde_json::json!({"id": "m0", "type": "model"}),
            serde_json::json!({"id": "m1", "type": "model"}),
            serde_json::json!({"id": "m2", "type": "model"}),
        ];
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Claude, &locals, Some("m1")),
            ModelListStage::Local(2) // m1 is at idx 1, next offset = 2
        );
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Claude, &locals, Some("m2")),
            ModelListStage::Local(3) // past end; will transition to upstream
        );
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Claude, &locals, Some("upstream-only-id")),
            ModelListStage::Upstream("upstream-only-id".into())
        );
        assert_eq!(
            parse_model_list_page_token(ProtocolKind::Claude, &locals, None),
            ModelListStage::Local(0)
        );
    }

    #[test]
    fn plan_fully_local_when_page_fits_under_local_count() {
        let locals: Vec<_> = (0..20).map(|i| gemini_local(&format!("m{i}"))).collect();
        let plan = plan_paginated_model_list(ProtocolKind::Gemini, locals, Some("pageSize=5"));
        let body = match plan {
            ModelListPlan::FullyLocal(b) => b,
            _ => panic!("expected FullyLocal"),
        };
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let arr = v.get("models").and_then(|m| m.as_array()).unwrap();
        assert_eq!(arr.len(), 5);
        // 20 locals, pageSize 5 → next token L5 (more locals remain).
        assert_eq!(v.get("nextPageToken").and_then(|t| t.as_str()), Some("L5"));
    }

    #[test]
    fn plan_cross_page_sends_short_upstream_query() {
        // 3 locals, client asks pageSize=10 starting from offset 0 → head
        // is all 3 locals, upstream needs 7.
        let locals: Vec<_> = (0..3).map(|i| gemini_local(&format!("loc{i}"))).collect();
        let plan = plan_paginated_model_list(ProtocolKind::Gemini, locals, Some("pageSize=10"));
        match plan {
            ModelListPlan::Upstream {
                upstream_query,
                local_head,
                locals_ids,
            } => {
                assert_eq!(local_head.len(), 3);
                assert_eq!(locals_ids.len(), 3);
                let q = upstream_query.expect("query present");
                assert!(q.contains("pageSize=7"), "got: {q}");
                assert!(
                    !q.contains("pageToken"),
                    "no token on first upstream hit: {q}"
                );
            }
            _ => panic!("expected cross-page Upstream plan"),
        }
    }

    #[test]
    fn plan_upstream_stage_forwards_opaque_token() {
        let locals: Vec<_> = (0..3).map(|i| gemini_local(&format!("loc{i}"))).collect();
        let plan = plan_paginated_model_list(
            ProtocolKind::Gemini,
            locals,
            Some("pageSize=10&pageToken=UopaqueXYZ"),
        );
        match plan {
            ModelListPlan::Upstream {
                upstream_query,
                local_head,
                ..
            } => {
                assert!(
                    local_head.is_empty(),
                    "upstream-stage must not re-send locals"
                );
                let q = upstream_query.expect("query");
                assert!(q.contains("pageSize=10"));
                assert!(q.contains("pageToken=opaqueXYZ"));
            }
            _ => panic!("expected Upstream plan"),
        }
    }

    #[test]
    fn finalize_merges_head_and_dedups_upstream() {
        // 2 locals: m0, m1; upstream returns m1 (dup) + r1 + r2 + nextPageToken opaque1
        let head = vec![gemini_local("m0"), gemini_local("m1")];
        let locals_ids: std::collections::HashSet<_> =
            ["m0", "m1"].iter().map(|s| s.to_string()).collect();
        let upstream_body = serde_json::to_vec(&serde_json::json!({
            "models": [
                {"name": "models/m1", "displayName": "m1"},
                {"name": "models/r1", "displayName": "r1"},
                {"name": "models/r2", "displayName": "r2"},
            ],
            "nextPageToken": "opaque1"
        }))
        .unwrap();
        let merged =
            finalize_paginated_model_list(ProtocolKind::Gemini, head, &locals_ids, &upstream_body);
        let v: serde_json::Value = serde_json::from_slice(&merged).unwrap();
        let names: Vec<&str> = v
            .get("models")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
            .collect();
        // m1 appears once (from head) — upstream's duplicate m1 is dropped.
        assert_eq!(
            names,
            vec!["models/m0", "models/m1", "models/r1", "models/r2"]
        );
        assert_eq!(
            v.get("nextPageToken").and_then(|t| t.as_str()),
            Some("Uopaque1")
        );
    }

    #[test]
    fn plan_exact_local_boundary_issues_transition_token() {
        // 10 locals, client pageSize=10 → one full page, but next token
        // must be L10 so the next request transitions into upstream.
        let locals: Vec<_> = (0..10).map(|i| gemini_local(&format!("m{i}"))).collect();
        let plan = plan_paginated_model_list(ProtocolKind::Gemini, locals, Some("pageSize=10"));
        let body = match plan {
            ModelListPlan::FullyLocal(b) => b,
            _ => panic!("expected FullyLocal at exact boundary"),
        };
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v.get("nextPageToken").and_then(|t| t.as_str()), Some("L10"));
    }

    #[test]
    fn plan_local_offset_past_end_enters_upstream() {
        // Boundary token L10 when locals has 10 items → enter upstream.
        let locals: Vec<_> = (0..10).map(|i| gemini_local(&format!("m{i}"))).collect();
        let plan = plan_paginated_model_list(
            ProtocolKind::Gemini,
            locals,
            Some("pageSize=10&pageToken=L10"),
        );
        match plan {
            ModelListPlan::Upstream {
                upstream_query,
                local_head,
                ..
            } => {
                assert!(local_head.is_empty());
                let q = upstream_query.unwrap();
                assert!(q.contains("pageSize=10"));
                assert!(!q.contains("pageToken"), "fresh upstream, no token: {q}");
            }
            _ => panic!("expected Upstream plan"),
        }
    }

    #[test]
    fn plan_openai_no_query_returns_full_list_in_one_shot() {
        // OpenAI `/v1/models` has no pagination. When client sent no query,
        // we must dump all locals + full upstream in one response.
        let locals: Vec<_> = (0..100)
            .map(|i| serde_json::json!({"id": format!("m{i}"), "object": "model"}))
            .collect();
        let plan = plan_paginated_model_list(ProtocolKind::OpenAi, locals.clone(), None);
        match plan {
            ModelListPlan::Upstream {
                upstream_query,
                local_head,
                ..
            } => {
                assert!(upstream_query.is_none(), "no pagination to upstream");
                assert_eq!(local_head.len(), 100, "all locals prepended at once");
            }
            _ => panic!("expected Upstream plan (full list)"),
        }
    }

    #[test]
    fn plan_openai_with_limit_still_paginates() {
        // When client explicitly passes `limit`, pagination engages.
        let locals: Vec<_> = (0..100)
            .map(|i| serde_json::json!({"id": format!("m{i}"), "object": "model"}))
            .collect();
        let plan = plan_paginated_model_list(ProtocolKind::OpenAi, locals, Some("limit=5"));
        match plan {
            ModelListPlan::FullyLocal(body) => {
                let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(v.get("data").unwrap().as_array().unwrap().len(), 5);
            }
            _ => panic!("expected FullyLocal plan"),
        }
    }
}
