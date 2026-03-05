use super::{
    AppState, Arc, Body, BuiltinChannel, Bytes, ChannelId, HttpError, MiddlewareTransformError,
    OperationFamily, ProtocolKind, ProviderDefinition, RequestAuthContext, Response,
    RetryWithPayloadRequest, RouteImplementation, RouteKey, SseToNdjsonRewriter, StatusCode,
    Stream, TokenizerResolutionContext, TransformRequest, TransformRequestPayload,
    TransformResponsePayload, UpstreamAndUsageEventInput, UpstreamError, UpstreamResponse,
    UpstreamStreamRecordContext, UpstreamStreamRecordGuard, apply_credential_update_and_persist,
    attach_usage_extractor, capture_tracked_http_events, claude_count_tokens_response,
    decode_response_for_usage, enqueue_credential_status_updates_for_request,
    enqueue_internal_tracked_http_events, enqueue_upstream_and_usage_event,
    enqueue_upstream_request_event_from_meta, gemini_count_tokens_response,
    is_wrapped_stream_channel, json, mpsc, ndjson_chunk_to_sse_chunk,
    normalize_upstream_response_body_for_channel,
    normalize_upstream_stream_ndjson_chunk_for_channel, now_unix_ms, openai_count_tokens_request,
    openai_count_tokens_response, resolve_provider_id, response_headers_to_pairs,
    try_local_response_for_channel, upstream_error_credential_id, upstream_error_request_meta,
    upstream_error_status, usage_request_context_from_payload,
    usage_request_context_from_transform_request,
};
use futures_util::StreamExt;

pub(super) fn should_rewrite_gemini_stream_to_ndjson(request: &TransformRequest) -> bool {
    matches!(
        request,
        TransformRequest::StreamGenerateContentGeminiNdjson(_)
    )
}

pub(super) fn is_sse_content_type(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type")
            && value.to_ascii_lowercase().contains("text/event-stream")
    })
}

pub(super) fn is_streaming_content_type(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type") && {
            let content_type = value.to_ascii_lowercase();
            content_type.contains("text/event-stream")
                || content_type.contains("application/x-ndjson")
        }
    })
}

pub(super) fn rewrite_content_type_to_ndjson(headers: &mut Vec<(String, String)>) {
    let mut replaced = false;
    for (name, value) in headers.iter_mut() {
        if name.eq_ignore_ascii_case("content-type") {
            *value = "application/x-ndjson".to_string();
            replaced = true;
        }
    }
    if !replaced {
        headers.push((
            "content-type".to_string(),
            "application/x-ndjson".to_string(),
        ));
    }
}

pub(super) fn remove_header_ignore_case(headers: &mut Vec<(String, String)>, header_name: &str) {
    headers.retain(|(name, _)| !name.eq_ignore_ascii_case(header_name));
}

pub(super) fn transformed_payload_content_type(
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> &'static str {
    if operation != OperationFamily::StreamGenerateContent {
        return "application/json";
    }
    match protocol {
        ProtocolKind::GeminiNDJson => "application/x-ndjson",
        _ => "text/event-stream",
    }
}

fn encode_transform_stream_error_chunk(protocol: ProtocolKind, message: String) -> Bytes {
    let payload = json!({
        "error": {
            "message": message,
            "type": "transform_serialization_error"
        }
    })
    .to_string();

    let chunk = if protocol == ProtocolKind::GeminiNDJson {
        format!("{payload}\n")
    } else {
        format!("event: error\ndata: {payload}\n\n")
    };
    Bytes::from(chunk)
}

pub(super) fn rewrite_content_type(headers: &mut Vec<(String, String)>, content_type: &str) {
    let mut replaced = false;
    for (name, value) in headers.iter_mut() {
        if name.eq_ignore_ascii_case("content-type") {
            *value = content_type.to_string();
            replaced = true;
        }
    }
    if !replaced {
        headers.push(("content-type".to_string(), content_type.to_string()));
    }
    remove_header_ignore_case(headers, "content-length");
}

pub(super) fn wrap_stream_with_upstream_record(
    input: std::pin::Pin<
        Box<dyn Stream<Item = Result<Bytes, MiddlewareTransformError>> + Send + 'static>,
    >,
    context: UpstreamStreamRecordContext,
) -> std::pin::Pin<Box<dyn Stream<Item = Result<Bytes, MiddlewareTransformError>> + Send + 'static>>
{
    let (tx, mut rx) = mpsc::channel::<Result<Bytes, MiddlewareTransformError>>(16);
    tokio::spawn(async move {
        let mut context = context;
        let mut input = input;
        if context.record_stream_usage_event {
            let usage_extracted = attach_usage_extractor(TransformResponsePayload::new(
                context.request.operation(),
                context.request.protocol(),
                input,
            ));
            context.stream_usage = Some(usage_extracted.usage.clone());
            input = usage_extracted.response.body;
        } else {
            context.stream_usage = None;
        }
        let recorder = UpstreamStreamRecordGuard::new(context);
        while let Some(item) = input.next().await {
            match item {
                Ok(chunk) => {
                    recorder.push_chunk(chunk.as_ref());
                    if tx
                        .send(Ok::<Bytes, MiddlewareTransformError>(chunk))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err::<Bytes, MiddlewareTransformError>(err)).await;
                    break;
                }
            }
        }
        recorder.flush_now().await;
    });
    Box::pin(async_stream::stream! {
        while let Some(item) = rx.recv().await {
            yield item;
        }
    })
}

pub(super) fn wrap_io_stream_with_upstream_record(
    input: std::pin::Pin<
        Box<dyn futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>,
    >,
    context: UpstreamStreamRecordContext,
) -> std::pin::Pin<
    Box<dyn futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>,
> {
    let (tx, mut rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(16);
    tokio::spawn(async move {
        let mut input: std::pin::Pin<
            Box<dyn Stream<Item = Result<Bytes, MiddlewareTransformError>> + Send + 'static>,
        > = Box::pin(input.map(|item| {
            item.map_err(|err| MiddlewareTransformError::ProviderPrefix {
                message: err.to_string(),
            })
        }));
        let mut context = context;
        if context.record_stream_usage_event {
            let usage_extracted = attach_usage_extractor(TransformResponsePayload::new(
                context.request.operation(),
                context.request.protocol(),
                input,
            ));
            context.stream_usage = Some(usage_extracted.usage.clone());
            input = usage_extracted.response.body;
        } else {
            context.stream_usage = None;
        }
        let recorder = UpstreamStreamRecordGuard::new(context);
        while let Some(item) = input.next().await {
            match item {
                Ok(chunk) => {
                    recorder.push_chunk(chunk.as_ref());
                    if tx.send(Ok::<Bytes, std::io::Error>(chunk)).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx
                        .send(Err::<Bytes, std::io::Error>(std::io::Error::other(
                            err.to_string(),
                        )))
                        .await;
                    break;
                }
            }
        }
        recorder.flush_now().await;
    });
    Box::pin(async_stream::stream! {
        while let Some(item) = rx.recv().await {
            yield item;
        }
    })
}

pub(super) fn unwrap_http_wrapper_body_bytes(body: &[u8]) -> Option<Vec<u8>> {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    let wrapper = value.as_object()?;
    if !wrapper.contains_key("stats_code") || !wrapper.contains_key("body") {
        return None;
    }
    match wrapper.get("body")? {
        serde_json::Value::String(text) => Some(text.as_bytes().to_vec()),
        body => serde_json::to_vec(body).ok(),
    }
}

pub(super) async fn transformed_payload_to_axum_response(
    status: StatusCode,
    mut headers: Vec<(String, String)>,
    payload: TransformResponsePayload,
    stream_record_context: Option<UpstreamStreamRecordContext>,
) -> Result<Response, UpstreamError> {
    let content_type = transformed_payload_content_type(payload.operation, payload.protocol);
    rewrite_content_type(&mut headers, content_type);
    let mut builder = Response::builder().status(status);
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    if payload.operation != OperationFamily::StreamGenerateContent {
        let mut body = payload.body;
        let mut collected = Vec::new();
        while let Some(item) = body.next().await {
            let chunk = item.map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
            collected.extend_from_slice(chunk.as_ref());
        }
        let client_body = unwrap_http_wrapper_body_bytes(collected.as_slice()).unwrap_or(collected);
        return builder
            .body(Body::from(client_body))
            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()));
    }
    let body = if let Some(context) = stream_record_context {
        wrap_stream_with_upstream_record(payload.body, context)
    } else {
        payload.body
    };
    let protocol = payload.protocol;
    let body_stream = body.map(move |item| match item {
        Ok(chunk) => Ok::<Bytes, std::io::Error>(chunk),
        Err(err) => Ok::<Bytes, std::io::Error>(encode_transform_stream_error_chunk(
            protocol,
            err.to_string(),
        )),
    });
    builder
        .body(Body::from_stream(body_stream))
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))
}

pub(super) fn response_from_status_headers_and_bytes(
    status: StatusCode,
    headers: &[(String, String)],
    body: Vec<u8>,
) -> Result<Response, UpstreamError> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder
        .body(Body::from(body))
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))
}

pub(super) fn ensure_stream_usage_option_on_native_chat(request: &mut TransformRequest) {
    if let TransformRequest::StreamGenerateContentOpenAiChatCompletions(value) = request {
        let options = value
            .body
            .stream_options
            .get_or_insert_with(Default::default);
        options.include_usage = Some(true);
    }
}

pub(super) fn encode_http_response_for_transform(
    status: StatusCode,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<Vec<u8>, UpstreamError> {
    let mut header_map = serde_json::Map::new();
    for (name, value) in headers {
        header_map.insert(name.clone(), serde_json::Value::String(value.clone()));
    }
    let body_json = serde_json::from_slice::<serde_json::Value>(body)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(body).to_string()));
    serde_json::to_vec(&json!({
        "stats_code": status.as_u16(),
        "headers": header_map,
        "body": body_json,
    }))
    .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))
}

pub(super) fn upstream_response_to_axum_stream(
    response: wreq::Response,
    rewrite_gemini_stream_to_ndjson: bool,
    stream_record_context: Option<UpstreamStreamRecordContext>,
) -> Result<Response, UpstreamError> {
    let stream_channel = stream_record_context
        .as_ref()
        .map(|value| value.channel.clone());
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect::<Vec<_>>();

    let is_sse = is_sse_content_type(headers.as_slice());
    let rewrite_stream = rewrite_gemini_stream_to_ndjson && is_sse;
    let unwrap_sse = !rewrite_stream
        && is_sse
        && stream_channel
            .as_ref()
            .map(is_wrapped_stream_channel)
            .unwrap_or(false);
    if rewrite_stream {
        rewrite_content_type_to_ndjson(&mut headers);
        remove_header_ignore_case(&mut headers, "content-length");
    } else if unwrap_sse {
        remove_header_ignore_case(&mut headers, "content-length");
    }

    let mut builder = Response::builder().status(status);
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }

    if rewrite_stream || unwrap_sse {
        let mut upstream_stream = response.bytes_stream();
        let mut rewriter = SseToNdjsonRewriter::default();
        let base_stream = async_stream::stream! {
            while let Some(item) = upstream_stream.next().await {
                let chunk = match item {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        yield Err::<Bytes, std::io::Error>(std::io::Error::other(err.to_string()));
                        return;
                    }
                };
                let out = rewriter.push_chunk(chunk.as_ref());
                if !out.is_empty() {
                    let normalized = stream_channel
                        .as_ref()
                        .and_then(|channel| {
                            normalize_upstream_stream_ndjson_chunk_for_channel(channel, out.as_slice())
                        })
                        .unwrap_or(out);
                    let output = if rewrite_stream {
                        normalized
                    } else {
                        ndjson_chunk_to_sse_chunk(normalized.as_slice())
                    };
                    if !output.is_empty() {
                        yield Ok::<Bytes, std::io::Error>(Bytes::from(output));
                    }
                }
            }
            let tail = rewriter.finish();
            if !tail.is_empty() {
                let normalized_tail = stream_channel
                    .as_ref()
                    .and_then(|channel| {
                        normalize_upstream_stream_ndjson_chunk_for_channel(
                            channel,
                            tail.as_slice(),
                        )
                    })
                    .unwrap_or(tail);
                let output_tail = if rewrite_stream {
                    normalized_tail
                } else {
                    ndjson_chunk_to_sse_chunk(normalized_tail.as_slice())
                };
                if !output_tail.is_empty() {
                    yield Ok::<Bytes, std::io::Error>(Bytes::from(output_tail));
                }
            }
        };
        let body_stream = if let Some(context) = stream_record_context {
            wrap_io_stream_with_upstream_record(Box::pin(base_stream), context)
        } else {
            Box::pin(base_stream)
        };
        return builder
            .body(Body::from_stream(body_stream))
            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()));
    }

    let base_body_stream = response.bytes_stream().map(|item| {
        item.map_err(|err| MiddlewareTransformError::ProviderPrefix {
            message: err.to_string(),
        })
    });
    let base_body_stream = if let Some(context) = stream_record_context {
        wrap_stream_with_upstream_record(Box::pin(base_body_stream), context)
    } else {
        Box::pin(base_body_stream)
    }
    .map(|item| item.map_err(|err| std::io::Error::other(err.to_string())));
    builder
        .body(Body::from_stream(base_body_stream))
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))
}

pub(super) fn build_openai_local_count_response(
    input_tokens: u64,
) -> openai_count_tokens_response::OpenAiCountTokensResponse {
    openai_count_tokens_response::OpenAiCountTokensResponse::Success {
        stats_code: StatusCode::OK,
        headers: gproxy_protocol::openai::types::OpenAiResponseHeaders::default(),
        body: openai_count_tokens_response::ResponseBody {
            input_tokens,
            object: openai_count_tokens_response::OpenAiCountTokensObject::ResponseInputTokens,
        },
    }
}

fn serialize_local_response_body(
    response: &gproxy_middleware::TransformResponse,
) -> Result<Vec<u8>, UpstreamError> {
    let value = serde_json::to_value(response)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    let inner = match value {
        serde_json::Value::Object(object) if object.len() == 1 => {
            if let Some((_, inner)) = object.into_iter().next() {
                inner
            } else {
                return Ok(Vec::new());
            }
        }
        other => other,
    };

    if let serde_json::Value::Object(wrapper) = &inner
        && wrapper.contains_key("stats_code")
        && wrapper.contains_key("body")
    {
        let body = wrapper
            .get("body")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        return match body {
            serde_json::Value::String(text) => Ok(text.into_bytes()),
            other => serde_json::to_vec(&other)
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string())),
        };
    }

    serde_json::to_vec(&inner).map_err(|err| UpstreamError::SerializeRequest(err.to_string()))
}

pub(super) async fn execute_local_count_token_request(
    state: &AppState,
    request: &TransformRequest,
) -> Result<UpstreamResponse, UpstreamError> {
    let openai_request = match request {
        TransformRequest::CountTokenOpenAi(value) => value.clone(),
        TransformRequest::CountTokenClaude(value) => {
            openai_count_tokens_request::OpenAiCountTokensRequest::try_from(value.clone())
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?
        }
        TransformRequest::CountTokenGemini(value) => {
            openai_count_tokens_request::OpenAiCountTokensRequest::try_from(value.clone())
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?
        }
        _ => return Err(UpstreamError::UnsupportedRequest),
    };

    let mut normalized = serde_json::to_value(&openai_request.body)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    if let Some(object) = normalized.as_object_mut() {
        object.remove("model");
    }
    let text = serde_json::to_string(&normalized)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    let model = openai_request
        .body
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("deepseek_fallback");

    let token_count = state
        .count_tokens_with_local_tokenizer(model, text.as_str())
        .await
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?
        .count as u64;

    let response = match request {
        TransformRequest::CountTokenOpenAi(_) => {
            gproxy_middleware::TransformResponse::CountTokenOpenAi(
                build_openai_local_count_response(token_count),
            )
        }
        TransformRequest::CountTokenClaude(_) => {
            gproxy_middleware::TransformResponse::CountTokenClaude(
                claude_count_tokens_response::ClaudeCountTokensResponse::try_from(
                    build_openai_local_count_response(token_count),
                )
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?,
            )
        }
        TransformRequest::CountTokenGemini(_) => {
            gproxy_middleware::TransformResponse::CountTokenGemini(
                gemini_count_tokens_response::GeminiCountTokensResponse::try_from(
                    build_openai_local_count_response(token_count),
                )
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?,
            )
        }
        _ => return Err(UpstreamError::UnsupportedRequest),
    };

    Ok(UpstreamResponse::from_local(response))
}

pub(super) async fn execute_local_request(
    state: &AppState,
    channel: &ChannelId,
    request: &TransformRequest,
) -> Result<UpstreamResponse, UpstreamError> {
    if let Some(local) = try_local_response_for_channel(channel, request)? {
        return Ok(UpstreamResponse::from_local(local));
    }

    execute_local_count_token_request(state, request).await
}

pub(super) async fn execute_transform_request(
    state: Arc<AppState>,
    channel: ChannelId,
    provider: ProviderDefinition,
    auth: RequestAuthContext,
    request: TransformRequest,
) -> Result<Response, UpstreamError> {
    let downstream_request = request;
    let mut upstream_request = downstream_request.clone();
    let downstream_request_context =
        usage_request_context_from_transform_request(&downstream_request);
    let mut dispatch_route = None;
    let mut dispatch_local = false;
    let provider_id = resolve_provider_id(state.as_ref(), &channel).await.ok();
    let src_route = RouteKey::new(
        downstream_request.operation(),
        downstream_request.protocol(),
    );
    let Some(implementation) = provider.dispatch.resolve(src_route).cloned() else {
        enqueue_upstream_and_usage_event(
            state.as_ref(),
            UpstreamAndUsageEventInput {
                auth,
                request: &downstream_request_context,
                provider_id,
                credential_id: None,
                request_meta: None,
                error_status: None,
                response_status: None,
                response_headers: &[],
                response_body: None,
                local_response: None,
            },
        )
        .await;
        return Err(UpstreamError::UnsupportedRequest);
    };

    match implementation {
        RouteImplementation::Passthrough => {}
        RouteImplementation::TransformTo { destination } => {
            let route = gproxy_middleware::TransformRoute {
                src_operation: src_route.operation,
                src_protocol: src_route.protocol,
                dst_operation: destination.operation,
                dst_protocol: destination.protocol,
            };
            if gproxy_middleware::select_request_lane(route)
                == gproxy_middleware::TransformLane::Typed
            {
                match gproxy_middleware::transform_request(downstream_request.clone(), route) {
                    Ok(transformed) => {
                        upstream_request = transformed;
                    }
                    Err(err) => {
                        let upstream_error = UpstreamError::SerializeRequest(err.to_string());
                        enqueue_upstream_and_usage_event(
                            state.as_ref(),
                            UpstreamAndUsageEventInput {
                                auth,
                                request: &downstream_request_context,
                                provider_id,
                                credential_id: None,
                                request_meta: None,
                                error_status: None,
                                response_status: None,
                                response_headers: &[],
                                response_body: None,
                                local_response: None,
                            },
                        )
                        .await;
                        return Err(upstream_error);
                    }
                }
            }
            dispatch_route = Some(route);
        }
        RouteImplementation::Local => {
            dispatch_local = true;
        }
        RouteImplementation::Unsupported => {
            enqueue_upstream_and_usage_event(
                state.as_ref(),
                UpstreamAndUsageEventInput {
                    auth,
                    request: &downstream_request_context,
                    provider_id,
                    credential_id: None,
                    request_meta: None,
                    error_status: None,
                    response_status: None,
                    response_headers: &[],
                    response_body: None,
                    local_response: None,
                },
            )
            .await;
            return Err(UpstreamError::UnsupportedRequest);
        }
    }

    let now = now_unix_ms();
    ensure_stream_usage_option_on_native_chat(&mut upstream_request);
    let upstream_request_context = usage_request_context_from_transform_request(&upstream_request);
    let (upstream_result, tracked_http_events) = if dispatch_local {
        (
            execute_local_request(state.as_ref(), &channel, &downstream_request).await,
            Vec::new(),
        )
    } else {
        let http = state.load_http();
        let spoof_http = matches!(&channel, ChannelId::Builtin(BuiltinChannel::ClaudeCode))
            .then(|| state.load_spoof_http());
        let tokenizers = state.tokenizers();
        let global = state.config.load().global.clone();

        capture_tracked_http_events(async {
            provider
                .execute_with_retry_with_spoof(
                    http.as_ref(),
                    spoof_http.as_deref(),
                    &state.credential_states,
                    &upstream_request,
                    now,
                    TokenizerResolutionContext {
                        tokenizer_store: tokenizers.as_ref(),
                        hf_token: global.hf_token.as_deref(),
                        hf_url: global.hf_url.as_deref(),
                    },
                )
                .await
        })
        .await
    };
    if !dispatch_local {
        enqueue_credential_status_updates_for_request(state.as_ref(), &channel, &provider, now)
            .await;
    }
    let upstream = match upstream_result {
        Ok(value) => value,
        Err(err) => {
            let err_request_meta = upstream_error_request_meta(&err);
            let err_credential_id = upstream_error_credential_id(&err);
            let err_status = upstream_error_status(&err);
            if !dispatch_local {
                enqueue_internal_tracked_http_events(
                    state.as_ref(),
                    auth.downstream_trace_id,
                    provider_id,
                    err_credential_id,
                    tracked_http_events.as_slice(),
                    err_request_meta.as_ref(),
                )
                .await;
            }
            enqueue_upstream_and_usage_event(
                state.as_ref(),
                UpstreamAndUsageEventInput {
                    auth,
                    request: &downstream_request_context,
                    provider_id,
                    credential_id: err_credential_id,
                    request_meta: err_request_meta.as_ref(),
                    error_status: err_status,
                    response_status: None,
                    response_headers: &[],
                    response_body: None,
                    local_response: None,
                },
            )
            .await;
            return Err(err);
        }
    };
    let upstream_credential_id = upstream.credential_id;
    let upstream_request_meta = upstream.request_meta.clone();
    if !dispatch_local {
        enqueue_internal_tracked_http_events(
            state.as_ref(),
            auth.downstream_trace_id,
            provider_id,
            upstream_credential_id,
            tracked_http_events.as_slice(),
            upstream_request_meta.as_ref(),
        )
        .await;
    }

    if let Some(update) = upstream.credential_update.clone() {
        apply_credential_update_and_persist(
            state.clone(),
            channel.clone(),
            provider.clone(),
            update,
        )
        .await;
    }

    if let Some(mut local) = upstream.local_response {
        let usage_source_response = local.clone();
        if let Some(route) = dispatch_route.filter(|item| {
            gproxy_middleware::select_response_lane(*item)
                == gproxy_middleware::TransformLane::Typed
        }) {
            local = gproxy_middleware::transform_response(local, route)
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
        }
        enqueue_upstream_and_usage_event(
            state.as_ref(),
            UpstreamAndUsageEventInput {
                auth,
                request: &upstream_request_context,
                provider_id,
                credential_id: upstream_credential_id,
                request_meta: upstream_request_meta.as_ref(),
                error_status: None,
                response_status: Some(200),
                response_headers: &[],
                response_body: None,
                local_response: Some(&usage_source_response),
            },
        )
        .await;
        let body = serialize_local_response_body(&local)?;
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
        return Ok(response);
    }

    if let Some(response) = upstream.response {
        let response_status = response.status().as_u16();
        let response_headers = response_headers_to_pairs(&response);
        if let Some(route) = dispatch_route.filter(|item| {
            gproxy_middleware::select_response_lane(*item)
                == gproxy_middleware::TransformLane::Typed
        }) {
            if !response.status().is_success() {
                let stream_record_context = UpstreamStreamRecordContext {
                    state: state.clone(),
                    channel: channel.clone(),
                    provider: provider.clone(),
                    auth,
                    request: upstream_request_context.clone(),
                    provider_id,
                    credential_id: upstream_credential_id,
                    request_meta: upstream_request_meta.clone(),
                    response_status: Some(response_status),
                    response_headers: response_headers.clone(),
                    stream_usage: None,
                    record_upstream_event: true,
                    record_stream_usage_event: true,
                };
                return upstream_response_to_axum_stream(
                    response,
                    false,
                    Some(stream_record_context),
                );
            }
            let status =
                StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|value| (name.as_str().to_string(), value.to_string()))
                })
                .collect::<Vec<_>>();
            let transformed_payload = if route.dst_operation
                == OperationFamily::StreamGenerateContent
            {
                let body_stream = response.bytes_stream().map(|item| {
                    item.map_err(|err| MiddlewareTransformError::ProviderPrefix {
                        message: err.to_string(),
                    })
                });
                let body_stream: std::pin::Pin<
                    Box<
                        dyn Stream<Item = Result<Bytes, MiddlewareTransformError>> + Send + 'static,
                    >,
                > = if is_wrapped_stream_channel(&channel)
                    && matches!(
                        route.dst_protocol,
                        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson
                    ) {
                    let mut upstream_stream = Box::pin(body_stream);
                    let wrapped_channel = channel.clone();
                    let dst_protocol = route.dst_protocol;
                    Box::pin(async_stream::stream! {
                        let mut rewriter = SseToNdjsonRewriter::default();
                        while let Some(item) = upstream_stream.next().await {
                            let chunk = match item {
                                Ok(chunk) => chunk,
                                Err(err) => {
                                    yield Err::<Bytes, MiddlewareTransformError>(err);
                                    return;
                                }
                            };
                            let out = rewriter.push_chunk(chunk.as_ref());
                            if !out.is_empty() {
                                let normalized = normalize_upstream_stream_ndjson_chunk_for_channel(
                                    &wrapped_channel,
                                    out.as_slice(),
                                )
                                .unwrap_or(out);
                                let emitted = if dst_protocol == ProtocolKind::Gemini {
                                    ndjson_chunk_to_sse_chunk(normalized.as_slice())
                                } else {
                                    normalized
                                };
                                if !emitted.is_empty() {
                                    yield Ok::<Bytes, MiddlewareTransformError>(Bytes::from(emitted));
                                }
                            }
                        }
                        let tail = rewriter.finish();
                        if !tail.is_empty() {
                            let normalized_tail = normalize_upstream_stream_ndjson_chunk_for_channel(
                                &wrapped_channel,
                                tail.as_slice(),
                            )
                            .unwrap_or(tail);
                            let emitted_tail = if dst_protocol == ProtocolKind::Gemini {
                                ndjson_chunk_to_sse_chunk(normalized_tail.as_slice())
                            } else {
                                normalized_tail
                            };
                            if !emitted_tail.is_empty() {
                                yield Ok::<Bytes, MiddlewareTransformError>(Bytes::from(emitted_tail));
                            }
                        }
                    })
                } else {
                    Box::pin(body_stream)
                };
                // Both upstream logs and usage capture are emitted after upstream normalization,
                // but before cross-protocol transform.
                let stream_record_context = UpstreamStreamRecordContext {
                    state: state.clone(),
                    channel: channel.clone(),
                    provider: provider.clone(),
                    auth,
                    request: upstream_request_context.clone(),
                    provider_id,
                    credential_id: upstream_credential_id,
                    request_meta: upstream_request_meta.clone(),
                    response_status: Some(response_status),
                    response_headers: response_headers.clone(),
                    stream_usage: None,
                    record_upstream_event: true,
                    record_stream_usage_event: true,
                };
                let body_stream =
                    wrap_stream_with_upstream_record(body_stream, stream_record_context);
                gproxy_middleware::transform_response_payload(
                    TransformResponsePayload::new(
                        route.dst_operation,
                        route.dst_protocol,
                        body_stream,
                    ),
                    route,
                )
                .await
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?
            } else {
                let body_bytes = response
                    .bytes()
                    .await
                    .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
                let raw_body = body_bytes.to_vec();
                let normalized_body =
                    normalize_upstream_response_body_for_channel(&channel, body_bytes.as_ref())
                        .unwrap_or_else(|| raw_body.clone());
                let encoded = encode_http_response_for_transform(
                    status,
                    headers.as_slice(),
                    normalized_body.as_ref(),
                )?;
                let usage_source_response = decode_response_for_usage(
                    route.dst_operation,
                    route.dst_protocol,
                    encoded.as_ref(),
                );
                enqueue_upstream_and_usage_event(
                    state.as_ref(),
                    UpstreamAndUsageEventInput {
                        auth,
                        request: &upstream_request_context,
                        provider_id,
                        credential_id: upstream_credential_id,
                        request_meta: upstream_request_meta.as_ref(),
                        error_status: None,
                        response_status: Some(response_status),
                        response_headers: response_headers.as_slice(),
                        response_body: Some(raw_body),
                        local_response: usage_source_response.as_ref(),
                    },
                )
                .await;
                let body_stream =
                    futures_util::stream::once(async move { Ok(Bytes::from(encoded)) });
                gproxy_middleware::transform_response_payload(
                    TransformResponsePayload::new(
                        route.dst_operation,
                        route.dst_protocol,
                        Box::pin(body_stream),
                    ),
                    route,
                )
                .await
                .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?
            };
            return transformed_payload_to_axum_response(
                status,
                headers,
                transformed_payload,
                None,
            )
            .await;
        }
        if should_rewrite_gemini_stream_to_ndjson(&downstream_request)
            || is_streaming_content_type(response_headers.as_slice())
        {
            let stream_record_context = UpstreamStreamRecordContext {
                state: state.clone(),
                channel: channel.clone(),
                provider: provider.clone(),
                auth,
                request: upstream_request_context.clone(),
                provider_id,
                credential_id: upstream_credential_id,
                request_meta: upstream_request_meta.clone(),
                response_status: Some(response_status),
                response_headers: response_headers.clone(),
                stream_usage: None,
                record_upstream_event: true,
                record_stream_usage_event: true,
            };
            return upstream_response_to_axum_stream(
                response,
                should_rewrite_gemini_stream_to_ndjson(&downstream_request),
                Some(stream_record_context),
            );
        }

        let status =
            StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect::<Vec<_>>();
        let body_bytes = response
            .bytes()
            .await
            .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
        let raw_body = body_bytes.to_vec();
        let normalized_body =
            normalize_upstream_response_body_for_channel(&channel, body_bytes.as_ref())
                .unwrap_or_else(|| raw_body.clone());
        let encoded_for_usage = encode_http_response_for_transform(
            status,
            headers.as_slice(),
            normalized_body.as_ref(),
        )?;
        let usage_source_response = decode_response_for_usage(
            upstream_request.operation(),
            upstream_request.protocol(),
            encoded_for_usage.as_ref(),
        );
        enqueue_upstream_and_usage_event(
            state.as_ref(),
            UpstreamAndUsageEventInput {
                auth,
                request: &upstream_request_context,
                provider_id,
                credential_id: upstream_credential_id,
                request_meta: upstream_request_meta.as_ref(),
                error_status: None,
                response_status: Some(response_status),
                response_headers: response_headers.as_slice(),
                response_body: Some(raw_body.clone()),
                local_response: usage_source_response.as_ref(),
            },
        )
        .await;
        let mut headers_for_client = headers.clone();
        if normalized_body != raw_body {
            remove_header_ignore_case(&mut headers_for_client, "content-length");
        }
        return response_from_status_headers_and_bytes(
            status,
            headers_for_client.as_slice(),
            normalized_body,
        );
    }

    enqueue_upstream_and_usage_event(
        state.as_ref(),
        UpstreamAndUsageEventInput {
            auth,
            request: &upstream_request_context,
            provider_id,
            credential_id: upstream_credential_id,
            request_meta: upstream_request_meta.as_ref(),
            error_status: None,
            response_status: None,
            response_headers: &[],
            response_body: None,
            local_response: None,
        },
    )
    .await;
    Err(UpstreamError::UpstreamRequest(
        "upstream returned empty response".to_string(),
    ))
}

async fn collect_request_payload_body_bytes(
    request: TransformRequestPayload,
) -> Result<(OperationFamily, ProtocolKind, Vec<u8>), UpstreamError> {
    let mut out = Vec::new();
    let mut body = request.body;
    while let Some(item) = body.next().await {
        let chunk = item.map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
        out.extend_from_slice(chunk.as_ref());
    }
    Ok((request.operation, request.protocol, out))
}

pub(super) fn shape_passthrough_request(
    _channel: &ChannelId,
    _operation: OperationFamily,
    _protocol: ProtocolKind,
    body_bytes: Vec<u8>,
) -> Vec<u8> {
    body_bytes
}

pub(super) fn shape_passthrough_response(
    channel: &ChannelId,
    _operation: OperationFamily,
    _protocol: ProtocolKind,
    _status: StatusCode,
    headers: &[(String, String)],
    body_bytes: Vec<u8>,
) -> (Vec<(String, String)>, Vec<u8>) {
    let normalized_body =
        normalize_upstream_response_body_for_channel(channel, body_bytes.as_ref())
            .unwrap_or_else(|| body_bytes.clone());
    let mut normalized_headers = headers.to_vec();
    if normalized_body != body_bytes {
        remove_header_ignore_case(&mut normalized_headers, "content-length");
    }
    (normalized_headers, normalized_body)
}

async fn execute_passthrough_payload_request(
    state: Arc<AppState>,
    channel: ChannelId,
    provider: ProviderDefinition,
    auth: RequestAuthContext,
    request: TransformRequestPayload,
) -> Result<Response, UpstreamError> {
    let (operation, protocol, request_bytes) = collect_request_payload_body_bytes(request).await?;
    let request_bytes = shape_passthrough_request(&channel, operation, protocol, request_bytes);
    let request_context =
        usage_request_context_from_payload(operation, protocol, request_bytes.as_slice());
    let provider_id = resolve_provider_id(state.as_ref(), &channel).await.ok();
    let now = now_unix_ms();
    let http = state.load_http();
    let spoof_http = matches!(&channel, ChannelId::Builtin(BuiltinChannel::ClaudeCode))
        .then(|| state.load_spoof_http());
    let tokenizers = state.tokenizers();
    let global = state.config.load().global.clone();

    let (upstream_result, tracked_http_events) = capture_tracked_http_events(async {
        provider
            .execute_payload_with_retry_with_spoof(
                http.as_ref(),
                spoof_http.as_deref(),
                &state.credential_states,
                RetryWithPayloadRequest {
                    operation,
                    protocol,
                    body: request_bytes.as_slice(),
                    now_unix_ms: now,
                    token_resolution: TokenizerResolutionContext {
                        tokenizer_store: tokenizers.as_ref(),
                        hf_token: global.hf_token.as_deref(),
                        hf_url: global.hf_url.as_deref(),
                    },
                },
            )
            .await
    })
    .await;

    enqueue_credential_status_updates_for_request(state.as_ref(), &channel, &provider, now).await;

    let upstream = match upstream_result {
        Ok(value) => value,
        Err(err) => {
            let err_request_meta = upstream_error_request_meta(&err);
            let err_credential_id = upstream_error_credential_id(&err);
            let err_status = upstream_error_status(&err);
            enqueue_internal_tracked_http_events(
                state.as_ref(),
                auth.downstream_trace_id,
                provider_id,
                err_credential_id,
                tracked_http_events.as_slice(),
                err_request_meta.as_ref(),
            )
            .await;
            enqueue_upstream_request_event_from_meta(
                state.as_ref(),
                auth.downstream_trace_id,
                provider_id,
                err_credential_id,
                err_request_meta.as_ref(),
                super::UpstreamResponseMeta {
                    status: err_status,
                    headers: &[],
                    body: None,
                },
            )
            .await;
            return Err(err);
        }
    };

    let upstream_credential_id = upstream.credential_id;
    let upstream_request_meta = upstream.request_meta.clone();
    enqueue_internal_tracked_http_events(
        state.as_ref(),
        auth.downstream_trace_id,
        provider_id,
        upstream_credential_id,
        tracked_http_events.as_slice(),
        upstream_request_meta.as_ref(),
    )
    .await;

    if let Some(update) = upstream.credential_update.clone() {
        apply_credential_update_and_persist(
            state.clone(),
            channel.clone(),
            provider.clone(),
            update,
        )
        .await;
    }

    if let Some(local) = upstream.local_response {
        let body = serialize_local_response_body(&local)?;
        enqueue_upstream_and_usage_event(
            state.as_ref(),
            UpstreamAndUsageEventInput {
                auth,
                request: &request_context,
                provider_id,
                credential_id: upstream_credential_id,
                request_meta: upstream_request_meta.as_ref(),
                error_status: None,
                response_status: Some(200),
                response_headers: &[("content-type".to_string(), "application/json".to_string())],
                response_body: None,
                local_response: Some(&local),
            },
        )
        .await;
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
        return Ok(response);
    }

    if let Some(response) = upstream.response {
        let response_status = response.status().as_u16();
        let response_headers = response_headers_to_pairs(&response);
        if operation == OperationFamily::StreamGenerateContent
            || is_streaming_content_type(response_headers.as_slice())
        {
            let rewrite_gemini_stream_to_ndjson = operation
                == OperationFamily::StreamGenerateContent
                && protocol == ProtocolKind::GeminiNDJson;
            let stream_record_context = UpstreamStreamRecordContext {
                state: state.clone(),
                channel: channel.clone(),
                provider: provider.clone(),
                auth,
                request: request_context.clone(),
                provider_id,
                credential_id: upstream_credential_id,
                request_meta: upstream_request_meta.clone(),
                response_status: Some(response_status),
                response_headers: response_headers.clone(),
                stream_usage: None,
                record_upstream_event: true,
                record_stream_usage_event: true,
            };
            return upstream_response_to_axum_stream(
                response,
                rewrite_gemini_stream_to_ndjson,
                Some(stream_record_context),
            );
        }

        let status =
            StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect::<Vec<_>>();
        let body_bytes = response
            .bytes()
            .await
            .map_err(|err| UpstreamError::UpstreamRequest(err.to_string()))?;
        let raw_body = body_bytes.to_vec();
        let (headers_for_client, normalized_body) = shape_passthrough_response(
            &channel,
            operation,
            protocol,
            status,
            headers.as_slice(),
            raw_body.clone(),
        );
        let encoded_for_usage = encode_http_response_for_transform(
            status,
            headers_for_client.as_slice(),
            normalized_body.as_ref(),
        )?;
        let usage_source_response =
            decode_response_for_usage(operation, protocol, encoded_for_usage.as_ref());
        enqueue_upstream_and_usage_event(
            state.as_ref(),
            UpstreamAndUsageEventInput {
                auth,
                request: &request_context,
                provider_id,
                credential_id: upstream_credential_id,
                request_meta: upstream_request_meta.as_ref(),
                error_status: None,
                response_status: Some(response_status),
                response_headers: response_headers.as_slice(),
                response_body: Some(raw_body.clone()),
                local_response: usage_source_response.as_ref(),
            },
        )
        .await;
        return response_from_status_headers_and_bytes(
            status,
            headers_for_client.as_slice(),
            normalized_body,
        );
    }

    enqueue_upstream_and_usage_event(
        state.as_ref(),
        UpstreamAndUsageEventInput {
            auth,
            request: &request_context,
            provider_id,
            credential_id: upstream_credential_id,
            request_meta: upstream_request_meta.as_ref(),
            error_status: None,
            response_status: None,
            response_headers: &[],
            response_body: None,
            local_response: None,
        },
    )
    .await;
    Err(UpstreamError::UpstreamRequest(
        "upstream returned empty response".to_string(),
    ))
}

pub(super) async fn execute_transform_request_payload(
    state: Arc<AppState>,
    channel: ChannelId,
    provider: ProviderDefinition,
    auth: RequestAuthContext,
    request: TransformRequestPayload,
) -> Result<Response, UpstreamError> {
    let src_route = RouteKey::new(request.operation, request.protocol);
    let Some(implementation) = provider.dispatch.resolve(src_route).cloned() else {
        return Err(UpstreamError::UnsupportedRequest);
    };

    let route = match implementation {
        RouteImplementation::Passthrough => Some(gproxy_middleware::TransformRoute {
            src_operation: src_route.operation,
            src_protocol: src_route.protocol,
            dst_operation: src_route.operation,
            dst_protocol: src_route.protocol,
        }),
        RouteImplementation::TransformTo { destination } => {
            Some(gproxy_middleware::TransformRoute {
                src_operation: src_route.operation,
                src_protocol: src_route.protocol,
                dst_operation: destination.operation,
                dst_protocol: destination.protocol,
            })
        }
        RouteImplementation::Local => None,
        RouteImplementation::Unsupported => return Err(UpstreamError::UnsupportedRequest),
    };

    if let Some(route) = route
        && gproxy_middleware::select_request_lane(route) == gproxy_middleware::TransformLane::Raw
    {
        return execute_passthrough_payload_request(state, channel, provider, auth, request).await;
    }

    let (operation, protocol, request_bytes) = collect_request_payload_body_bytes(request).await?;
    let decoded =
        gproxy_middleware::decode_request_payload(operation, protocol, request_bytes.as_slice())
            .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    execute_transform_request(state, channel, provider, auth, decoded).await
}

pub(super) async fn execute_transform_candidates(
    state: Arc<AppState>,
    channel: ChannelId,
    provider: ProviderDefinition,
    auth: RequestAuthContext,
    candidates: Vec<TransformRequest>,
) -> Result<Response, HttpError> {
    let mut unsupported = false;
    for candidate in candidates {
        match execute_transform_request(
            state.clone(),
            channel.clone(),
            provider.clone(),
            auth,
            candidate,
        )
        .await
        {
            Ok(response) => return Ok(response),
            Err(UpstreamError::UnsupportedRequest) => {
                unsupported = true;
            }
            Err(err) => return Err(HttpError::from(err)),
        }
    }
    if unsupported {
        return Err(HttpError::from(UpstreamError::UnsupportedRequest));
    }
    Err(HttpError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "no provider route candidate executed",
    ))
}

#[cfg(test)]
mod tests {
    use gproxy_middleware::ProtocolKind;
    use gproxy_middleware::TransformResponse;
    use gproxy_protocol::openai::model_list::response::OpenAiModelListResponse;
    use serde_json::json;

    use super::{encode_transform_stream_error_chunk, serialize_local_response_body};

    #[test]
    fn local_response_body_is_unwrapped_from_enum_shell_and_http_wrapper() {
        let response: OpenAiModelListResponse = serde_json::from_value(json!({
            "stats_code": 200,
            "headers": {},
            "body": {
                "object": "list",
                "data": []
            }
        }))
        .expect("valid openai model list response");

        let bytes = serialize_local_response_body(&TransformResponse::ModelListOpenAi(response))
            .expect("serialize local response");
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("decode serialized local response");

        assert!(value.get("ModelListOpenAi").is_none());
        assert!(value.get("stats_code").is_none());
        assert_eq!(value.get("object").and_then(|v| v.as_str()), Some("list"));
        assert!(value.get("data").is_some());
    }

    #[test]
    fn stream_transform_error_chunk_is_ndjson_for_gemini_ndjson() {
        let chunk =
            encode_transform_stream_error_chunk(ProtocolKind::GeminiNDJson, "boom".to_string());
        let text = String::from_utf8(chunk.to_vec()).expect("utf8");
        assert!(text.ends_with('\n'));

        let value: serde_json::Value = serde_json::from_str(text.trim()).expect("json");
        assert_eq!(
            value
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str()),
            Some("boom")
        );
        assert_eq!(
            value
                .get("error")
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("transform_serialization_error")
        );
    }

    #[test]
    fn stream_transform_error_chunk_is_sse_for_non_ndjson() {
        let chunk = encode_transform_stream_error_chunk(ProtocolKind::OpenAi, "boom".to_string());
        let text = String::from_utf8(chunk.to_vec()).expect("utf8");
        assert!(text.starts_with("event: error\n"));
        assert!(text.ends_with("\n\n"));

        let data_line = text
            .lines()
            .find(|line| line.starts_with("data: "))
            .expect("data line");
        let payload = data_line.trim_start_matches("data: ");
        let value: serde_json::Value = serde_json::from_str(payload).expect("json");
        assert_eq!(
            value
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str()),
            Some("boom")
        );
    }
}
