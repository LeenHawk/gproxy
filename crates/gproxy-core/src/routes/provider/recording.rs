use super::{
    AppState, BetaUsage, BuiltinChannel, ChannelId, ClaudeModel, CompactResponseUsage,
    CompletionUsage, CredentialStatusWrite, GeminiUsageMetadata, OpenAiEmbeddingModel,
    OpenAiEmbeddingUsage, OperationFamily, ProtocolKind, ProviderDefinition, RequestAuthContext,
    ResponseInput, ResponseUsage, RouteImplementation, RouteKey, StorageWriteEvent, SystemTime,
    TokenizerResolutionContext, TrackedHttpEvent, TransformRequest, UNIX_EPOCH, UpstreamError,
    UpstreamRequestMeta, UpstreamRequestWrite, UpstreamStreamRecordContext, UsageRequestContext,
    UsageSnapshot, UsageWrite, claude_count_tokens_request, claude_count_tokens_response,
    claude_create_message_response, execute_local_count_token_request, gemini_count_tokens_request,
    gemini_count_tokens_response, gemini_generate_content_response,
    openai_chat_completions_response, openai_compact_response_response,
    openai_count_tokens_request, openai_count_tokens_response, openai_create_response_response,
    openai_embeddings_response,
};
use gproxy_provider::{
    credential_health_to_storage,
    normalize_upstream_response_body_for_channel as provider_normalize_upstream_response_body_for_channel,
    normalize_upstream_stream_ndjson_chunk_for_channel as provider_normalize_upstream_stream_ndjson_chunk_for_channel,
};

pub(super) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn now_unix_ms_i64() -> i64 {
    i64::try_from(now_unix_ms()).unwrap_or(i64::MAX)
}

pub(super) fn headers_pairs_to_json(headers: &[(String, String)]) -> String {
    let mut map: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (name, value) in headers {
        map.entry(name.clone()).or_default().push(value.clone());
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

pub(super) fn response_headers_to_pairs(response: &wreq::Response) -> Vec<(String, String)> {
    response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

pub(super) fn should_record_usage(operation: OperationFamily) -> bool {
    !matches!(
        operation,
        OperationFamily::ModelList | OperationFamily::ModelGet
    )
}

pub(super) fn upstream_error_request_meta(error: &UpstreamError) -> Option<UpstreamRequestMeta> {
    match error {
        UpstreamError::AllCredentialsExhausted {
            last_request_meta, ..
        } => last_request_meta.as_deref().cloned(),
        _ => None,
    }
}

pub(super) fn upstream_error_credential_id(error: &UpstreamError) -> Option<i64> {
    match error {
        UpstreamError::AllCredentialsExhausted {
            last_credential_id, ..
        } => *last_credential_id,
        _ => None,
    }
}

pub(super) fn upstream_error_status(error: &UpstreamError) -> Option<u16> {
    match error {
        UpstreamError::AllCredentialsExhausted { last_status, .. } => *last_status,
        _ => None,
    }
}

pub(super) async fn enqueue_credential_status_updates_for_request(
    state: &AppState,
    channel: &ChannelId,
    provider: &ProviderDefinition,
    request_now_unix_ms: u64,
) {
    for credential in provider.credentials.list_credentials() {
        let Some(state_row) = state.credential_states.get(channel, credential.id) else {
            continue;
        };
        if state_row.checked_at_unix_ms != Some(request_now_unix_ms) {
            continue;
        }

        let (health_kind, health_json) = credential_health_to_storage(&state_row.health);
        let checked_at_unix_ms = state_row
            .checked_at_unix_ms
            .and_then(|value| i64::try_from(value).ok());
        let event = StorageWriteEvent::UpsertCredentialStatus(CredentialStatusWrite {
            id: None,
            credential_id: credential.id,
            channel: channel.as_str().to_string(),
            health_kind,
            health_json,
            checked_at_unix_ms,
            last_error: state_row.last_error.clone(),
        });
        if let Err(err) = state.enqueue_storage_write(event).await {
            eprintln!(
                "provider: credential status enqueue failed channel={} credential_id={} error={}",
                channel.as_str(),
                credential.id,
                err
            );
        }
    }
}

pub(super) fn extract_local_count_input_tokens(
    response: &gproxy_middleware::TransformResponse,
) -> Option<i64> {
    match response {
        gproxy_middleware::TransformResponse::CountTokenOpenAi(
            openai_count_tokens_response::OpenAiCountTokensResponse::Success { body, .. },
        ) => i64::try_from(body.input_tokens).ok(),
        gproxy_middleware::TransformResponse::CountTokenClaude(
            claude_count_tokens_response::ClaudeCountTokensResponse::Success { body, .. },
        ) => i64::try_from(body.input_tokens).ok(),
        gproxy_middleware::TransformResponse::CountTokenGemini(
            gemini_count_tokens_response::GeminiCountTokensResponse::Success { body, .. },
        ) => i64::try_from(body.total_tokens).ok(),
        _ => None,
    }
}

pub(super) fn extract_count_tokens_from_raw_json(
    protocol: ProtocolKind,
    body: &[u8],
) -> Option<i64> {
    match protocol {
        ProtocolKind::OpenAi | ProtocolKind::OpenAiChatCompletion => {
            if let Ok(value) =
                serde_json::from_slice::<openai_count_tokens_response::ResponseBody>(body)
            {
                return i64::try_from(value.input_tokens).ok();
            }
            serde_json::from_slice::<openai_count_tokens_response::OpenAiCountTokensResponse>(body)
                .ok()
                .and_then(|value| match value {
                    openai_count_tokens_response::OpenAiCountTokensResponse::Success {
                        body,
                        ..
                    } => i64::try_from(body.input_tokens).ok(),
                    _ => None,
                })
        }
        ProtocolKind::Claude => {
            if let Ok(value) =
                serde_json::from_slice::<claude_count_tokens_response::ResponseBody>(body)
            {
                return i64::try_from(value.input_tokens).ok();
            }
            serde_json::from_slice::<claude_count_tokens_response::ClaudeCountTokensResponse>(body)
                .ok()
                .and_then(|value| match value {
                    claude_count_tokens_response::ClaudeCountTokensResponse::Success {
                        body,
                        ..
                    } => i64::try_from(body.input_tokens).ok(),
                    _ => None,
                })
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            if let Ok(value) =
                serde_json::from_slice::<gemini_count_tokens_response::ResponseBody>(body)
            {
                return i64::try_from(value.total_tokens).ok();
            }
            serde_json::from_slice::<gemini_count_tokens_response::GeminiCountTokensResponse>(body)
                .ok()
                .and_then(|value| match value {
                    gemini_count_tokens_response::GeminiCountTokensResponse::Success {
                        body,
                        ..
                    } => i64::try_from(body.total_tokens).ok(),
                    _ => None,
                })
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct UsageMetrics {
    pub(super) input_tokens: Option<i64>,
    pub(super) output_tokens: Option<i64>,
    pub(super) cache_read_input_tokens: Option<i64>,
    pub(super) cache_creation_input_tokens: Option<i64>,
    pub(super) cache_creation_input_tokens_5min: Option<i64>,
    pub(super) cache_creation_input_tokens_1h: Option<i64>,
}

pub(super) fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn usage_metrics_from_openai_response_usage(usage: &ResponseUsage) -> UsageMetrics {
    UsageMetrics {
        input_tokens: Some(u64_to_i64(usage.input_tokens)),
        output_tokens: Some(u64_to_i64(usage.output_tokens)),
        cache_read_input_tokens: Some(u64_to_i64(usage.input_tokens_details.cached_tokens)),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    }
}

pub(super) fn usage_metrics_from_openai_compact_usage(
    usage: &CompactResponseUsage,
) -> UsageMetrics {
    UsageMetrics {
        input_tokens: Some(u64_to_i64(usage.input_tokens)),
        output_tokens: Some(u64_to_i64(usage.output_tokens)),
        cache_read_input_tokens: Some(u64_to_i64(usage.input_tokens_details.cached_tokens)),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    }
}

pub(super) fn usage_metrics_from_openai_chat_completion_usage(
    usage: &CompletionUsage,
) -> UsageMetrics {
    UsageMetrics {
        input_tokens: Some(u64_to_i64(usage.prompt_tokens)),
        output_tokens: Some(u64_to_i64(usage.completion_tokens)),
        cache_read_input_tokens: usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|value| value.cached_tokens)
            .map(u64_to_i64),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    }
}

pub(super) fn usage_metrics_from_claude_usage(usage: &BetaUsage) -> UsageMetrics {
    let input_tokens = usage
        .input_tokens
        .saturating_add(usage.cache_creation_input_tokens)
        .saturating_add(usage.cache_read_input_tokens);
    UsageMetrics {
        input_tokens: Some(u64_to_i64(input_tokens)),
        output_tokens: Some(u64_to_i64(usage.output_tokens)),
        cache_read_input_tokens: Some(u64_to_i64(usage.cache_read_input_tokens)),
        cache_creation_input_tokens: Some(u64_to_i64(usage.cache_creation_input_tokens)),
        cache_creation_input_tokens_5min: Some(u64_to_i64(
            usage.cache_creation.ephemeral_5m_input_tokens,
        )),
        cache_creation_input_tokens_1h: Some(u64_to_i64(
            usage.cache_creation.ephemeral_1h_input_tokens,
        )),
    }
}

pub(super) fn usage_metrics_from_gemini_usage(usage: &GeminiUsageMetadata) -> UsageMetrics {
    let input_tokens = usage
        .prompt_token_count
        .unwrap_or(0)
        .saturating_add(usage.cached_content_token_count.unwrap_or(0));
    UsageMetrics {
        input_tokens: usage
            .prompt_token_count
            .or(usage.cached_content_token_count)
            .map(|_| u64_to_i64(input_tokens)),
        output_tokens: usage.candidates_token_count.map(u64_to_i64),
        cache_read_input_tokens: usage.cached_content_token_count.map(u64_to_i64),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    }
}

pub(super) fn usage_metrics_from_openai_embeddings_usage(
    usage: &OpenAiEmbeddingUsage,
) -> UsageMetrics {
    let prompt_tokens = u64_to_i64(usage.prompt_tokens);
    let total_tokens = u64_to_i64(usage.total_tokens);
    UsageMetrics {
        input_tokens: Some(prompt_tokens),
        output_tokens: Some(total_tokens.saturating_sub(prompt_tokens)),
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    }
}

pub(super) fn extract_usage_from_local_response(
    response: &gproxy_middleware::TransformResponse,
) -> Option<UsageMetrics> {
    match response {
        gproxy_middleware::TransformResponse::CountTokenOpenAi(
            openai_count_tokens_response::OpenAiCountTokensResponse::Success { body, .. },
        ) => Some(UsageMetrics {
            input_tokens: Some(u64_to_i64(body.input_tokens)),
            output_tokens: Some(0),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        }),
        gproxy_middleware::TransformResponse::CountTokenClaude(
            claude_count_tokens_response::ClaudeCountTokensResponse::Success { body, .. },
        ) => Some(UsageMetrics {
            input_tokens: Some(u64_to_i64(body.input_tokens)),
            output_tokens: Some(0),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        }),
        gproxy_middleware::TransformResponse::CountTokenGemini(
            gemini_count_tokens_response::GeminiCountTokensResponse::Success { body, .. },
        ) => Some(UsageMetrics {
            input_tokens: Some(u64_to_i64(body.total_tokens)),
            output_tokens: Some(0),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        }),
        gproxy_middleware::TransformResponse::GenerateContentOpenAiResponse(
            openai_create_response_response::OpenAiCreateResponseResponse::Success { body, .. },
        ) => body
            .usage
            .as_ref()
            .map(usage_metrics_from_openai_response_usage),
        gproxy_middleware::TransformResponse::GenerateContentOpenAiChatCompletions(
            openai_chat_completions_response::OpenAiChatCompletionsResponse::Success {
                body, ..
            },
        ) => body
            .usage
            .as_ref()
            .map(usage_metrics_from_openai_chat_completion_usage),
        gproxy_middleware::TransformResponse::GenerateContentClaude(
            claude_create_message_response::ClaudeCreateMessageResponse::Success { body, .. },
        ) => Some(usage_metrics_from_claude_usage(&body.usage)),
        gproxy_middleware::TransformResponse::GenerateContentGemini(
            gemini_generate_content_response::GeminiGenerateContentResponse::Success {
                body, ..
            },
        ) => body
            .usage_metadata
            .as_ref()
            .map(usage_metrics_from_gemini_usage),
        gproxy_middleware::TransformResponse::EmbeddingOpenAi(
            openai_embeddings_response::OpenAiEmbeddingsResponse::Success { body, .. },
        ) => Some(usage_metrics_from_openai_embeddings_usage(&body.usage)),
        gproxy_middleware::TransformResponse::CompactOpenAi(
            openai_compact_response_response::OpenAiCompactResponse::Success { body, .. },
        ) => Some(usage_metrics_from_openai_compact_usage(&body.usage)),
        _ => None,
    }
}

pub(super) fn decode_response_for_usage(
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: &[u8],
) -> Option<gproxy_middleware::TransformResponse> {
    gproxy_middleware::decode_response_payload(operation, protocol, body).ok()
}

pub(super) fn normalize_upstream_response_body_for_channel(
    channel: &ChannelId,
    body: &[u8],
) -> Option<Vec<u8>> {
    provider_normalize_upstream_response_body_for_channel(channel, body)
}

pub(super) fn normalize_upstream_stream_ndjson_chunk_for_channel(
    channel: &ChannelId,
    chunk: &[u8],
) -> Option<Vec<u8>> {
    provider_normalize_upstream_stream_ndjson_chunk_for_channel(channel, chunk)
}

pub(super) fn is_wrapped_stream_channel(channel: &ChannelId) -> bool {
    matches!(
        channel,
        ChannelId::Builtin(BuiltinChannel::GeminiCli)
            | ChannelId::Builtin(BuiltinChannel::Antigravity)
    )
}

pub(super) fn ndjson_chunk_to_sse_chunk(chunk: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(chunk) else {
        return chunk.to_vec();
    };
    let mut out = String::with_capacity(text.len().saturating_mul(2));
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        out.push_str("data: ");
        out.push_str(line);
        out.push_str("\n\n");
    }
    out.into_bytes()
}

pub(super) fn strip_model_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.retain(|key, _| !key.eq_ignore_ascii_case("model"));
            for child in object.values_mut() {
                strip_model_fields(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_model_fields(item);
            }
        }
        _ => {}
    }
}

pub(super) async fn estimate_embedding_input_tokens_from_usage_request(
    state: &AppState,
    request: &UsageRequestContext,
) -> Option<i64> {
    if request.operation() != OperationFamily::Embedding {
        return None;
    }
    let model = request.model.as_ref()?.trim().to_string();
    if model.is_empty() {
        return None;
    }
    let body = request.body_for_estimate.as_ref()?;
    let mut value = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    strip_model_fields(&mut value);
    let text = serde_json::to_string(&value).ok()?;
    let count = state
        .count_tokens_with_local_tokenizer(model.as_str(), text.as_str())
        .await
        .ok()?
        .count;
    i64::try_from(count).ok()
}

pub(super) fn build_openai_count_request(
    model: &str,
    text: &str,
) -> openai_count_tokens_request::OpenAiCountTokensRequest {
    let mut request = openai_count_tokens_request::OpenAiCountTokensRequest::default();
    request.body.model = Some(model.to_string());
    request.body.input = Some(ResponseInput::Text(text.to_string()));
    request
}

pub(super) fn normalize_count_source_text(source: &str) -> String {
    if source.trim().is_empty() {
        " ".to_string()
    } else {
        source.to_string()
    }
}

pub(super) async fn estimate_tokens_with_channel_count(
    context: &UpstreamStreamRecordContext,
    model: &str,
    text: &str,
) -> Option<i64> {
    let source = normalize_count_source_text(text);
    let openai_request = build_openai_count_request(model, source.as_str());
    let mut candidates = vec![(
        ProtocolKind::OpenAi,
        TransformRequest::CountTokenOpenAi(openai_request.clone()),
    )];
    if let Ok(request) =
        claude_count_tokens_request::ClaudeCountTokensRequest::try_from(openai_request.clone())
    {
        candidates.push((
            ProtocolKind::Claude,
            TransformRequest::CountTokenClaude(request),
        ));
    }
    if let Ok(request) =
        gemini_count_tokens_request::GeminiCountTokensRequest::try_from(openai_request)
    {
        candidates.push((
            ProtocolKind::Gemini,
            TransformRequest::CountTokenGemini(request),
        ));
    }

    for (source_protocol, source_request) in candidates {
        let source_route = RouteKey::new(OperationFamily::CountToken, source_protocol);
        let Some(implementation) = context.provider.dispatch.resolve(source_route).cloned() else {
            continue;
        };
        let mut upstream_request = source_request.clone();
        let mut upstream_protocol = source_protocol;
        let execute_local = match implementation {
            RouteImplementation::Unsupported => continue,
            RouteImplementation::Local => true,
            RouteImplementation::Passthrough => false,
            RouteImplementation::TransformTo { destination } => {
                let route = gproxy_middleware::TransformRoute {
                    src_operation: source_route.operation,
                    src_protocol: source_route.protocol,
                    dst_operation: destination.operation,
                    dst_protocol: destination.protocol,
                };
                if !route.is_passthrough() {
                    let Ok(transformed) =
                        gproxy_middleware::transform_request(upstream_request.clone(), route)
                    else {
                        continue;
                    };
                    upstream_request = transformed;
                }
                upstream_protocol = destination.protocol;
                false
            }
        };

        if execute_local {
            let Ok(local) =
                execute_local_count_token_request(context.state.as_ref(), &source_request).await
            else {
                continue;
            };
            let Some(local_response) = local.local_response.as_ref() else {
                continue;
            };
            if let Some(tokens) = extract_local_count_input_tokens(local_response) {
                return Some(tokens);
            }
            continue;
        }

        let now = now_unix_ms();
        let http = context.state.load_http();
        let spoof_http = matches!(
            &context.channel,
            ChannelId::Builtin(BuiltinChannel::ClaudeCode)
        )
        .then(|| context.state.load_spoof_http());
        let tokenizers = context.state.tokenizers();
        let global = context.state.config.load().global.clone();
        let Ok(upstream) = context
            .provider
            .execute_with_retry_with_spoof(
                http.as_ref(),
                spoof_http.as_deref(),
                &context.state.credential_states,
                &upstream_request,
                now,
                TokenizerResolutionContext {
                    tokenizer_store: tokenizers.as_ref(),
                    hf_token: global.hf_token.as_deref(),
                    hf_url: global.hf_url.as_deref(),
                },
            )
            .await
        else {
            continue;
        };

        if let Some(local_response) = upstream.local_response.as_ref()
            && let Some(tokens) = extract_local_count_input_tokens(local_response)
        {
            return Some(tokens);
        }

        let Some(response) = upstream.response else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if let Some(tokens) = extract_count_tokens_from_raw_json(upstream_protocol, bytes.as_ref())
        {
            return Some(tokens);
        }
    }

    None
}

pub(super) async fn estimate_tokens_for_text(
    context: &UpstreamStreamRecordContext,
    model: &str,
    text: &str,
) -> i64 {
    if let Some(tokens) = estimate_tokens_with_channel_count(context, model, text).await {
        return tokens.max(0);
    }
    context
        .state
        .count_tokens_with_local_tokenizer(model, text)
        .await
        .map(|count| i64::try_from(count.count).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

pub(super) async fn enqueue_stream_usage_event_with_estimate(
    context: &UpstreamStreamRecordContext,
    stream_response_body: &[u8],
    stream_usage: Option<UsageSnapshot>,
) {
    if !should_record_usage(context.request.operation())
        || context
            .response_status
            .map(|status| status >= 400)
            .unwrap_or(true)
    {
        return;
    }

    let request_model = normalize_usage_model(context.request.model.clone());
    let model = request_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("deepseek_fallback")
        .to_string();

    let mut usage = stream_usage;
    let mut input_tokens = usage
        .as_ref()
        .and_then(|value| value.input_tokens)
        .map(u64_to_i64);
    let mut output_tokens = usage
        .as_ref()
        .and_then(|value| value.output_tokens)
        .map(u64_to_i64);
    let mut cache_read_input_tokens = usage
        .as_ref()
        .and_then(|value| value.cache_read_input_tokens)
        .map(u64_to_i64);
    let mut cache_creation_input_tokens = usage
        .as_ref()
        .and_then(|value| value.cache_creation_input_tokens)
        .map(u64_to_i64);
    let mut cache_creation_input_tokens_5min = usage
        .as_ref()
        .and_then(|value| value.cache_creation_input_tokens_5min)
        .map(u64_to_i64);
    let mut cache_creation_input_tokens_1h = usage
        .as_ref()
        .and_then(|value| value.cache_creation_input_tokens_1h)
        .map(u64_to_i64);

    if let Some(total) = usage
        .as_ref()
        .and_then(|value| value.total_tokens)
        .map(u64_to_i64)
    {
        match (input_tokens, output_tokens) {
            (None, Some(output)) => {
                input_tokens = Some(total.saturating_sub(output));
            }
            (Some(input), None) => {
                output_tokens = Some(total.saturating_sub(input));
            }
            _ => {}
        }
    }

    if input_tokens.is_none()
        && output_tokens.is_none()
        && cache_read_input_tokens.is_none()
        && cache_creation_input_tokens.is_none()
        && cache_creation_input_tokens_5min.is_none()
        && cache_creation_input_tokens_1h.is_none()
    {
        usage = extract_usage_from_stream_body(
            context.request.operation(),
            context.request.protocol(),
            stream_response_body,
        )
        .await;
        if let Some(parsed_usage) = usage.as_ref() {
            input_tokens = parsed_usage.input_tokens.map(u64_to_i64);
            output_tokens = parsed_usage.output_tokens.map(u64_to_i64);
            cache_read_input_tokens = parsed_usage.cache_read_input_tokens.map(u64_to_i64);
            cache_creation_input_tokens = parsed_usage.cache_creation_input_tokens.map(u64_to_i64);
            cache_creation_input_tokens_5min = parsed_usage
                .cache_creation_input_tokens_5min
                .map(u64_to_i64);
            cache_creation_input_tokens_1h =
                parsed_usage.cache_creation_input_tokens_1h.map(u64_to_i64);
            if let Some(total) = parsed_usage.total_tokens.map(u64_to_i64) {
                match (input_tokens, output_tokens) {
                    (None, Some(output)) => {
                        input_tokens = Some(total.saturating_sub(output));
                    }
                    (Some(input), None) => {
                        output_tokens = Some(total.saturating_sub(input));
                    }
                    _ => {}
                }
            }
        }
    }

    if input_tokens.is_none()
        && output_tokens.is_none()
        && cache_read_input_tokens.is_none()
        && cache_creation_input_tokens.is_none()
        && cache_creation_input_tokens_5min.is_none()
        && cache_creation_input_tokens_1h.is_none()
    {
        let request_text = context
            .request
            .body_for_estimate
            .as_deref()
            .map(|body| {
                serde_json::from_slice::<serde_json::Value>(body)
                    .ok()
                    .and_then(|value| serde_json::to_string(&value).ok())
                    .unwrap_or_else(|| String::from_utf8_lossy(body).to_string())
            })
            .unwrap_or_default();
        let response_text = String::from_utf8_lossy(stream_response_body).to_string();

        input_tokens =
            Some(estimate_tokens_for_text(context, model.as_str(), request_text.as_str()).await);
        output_tokens =
            Some(estimate_tokens_for_text(context, model.as_str(), response_text.as_str()).await);
    }

    let usage_event = UsageWrite {
        downstream_trace_id: context.auth.downstream_trace_id,
        at_unix_ms: now_unix_ms_i64(),
        provider_id: context.provider_id,
        credential_id: context.credential_id,
        user_id: Some(context.auth.user_id),
        user_key_id: Some(context.auth.user_key_id),
        operation: format!("{:?}", context.request.operation()),
        protocol: format!("{:?}", context.request.protocol()),
        model: request_model,
        input_tokens: input_tokens.map(|value| value.max(0)),
        output_tokens: output_tokens.map(|value| value.max(0)),
        cache_read_input_tokens,
        cache_creation_input_tokens,
        cache_creation_input_tokens_5min,
        cache_creation_input_tokens_1h,
    };
    if let Err(err) = context
        .state
        .enqueue_storage_write(StorageWriteEvent::UpsertUsage(usage_event))
        .await
    {
        eprintln!("provider: stream usage event enqueue failed: {err}");
    }
}

async fn extract_usage_from_stream_body(
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: &[u8],
) -> Option<UsageSnapshot> {
    if body.is_empty() || operation != OperationFamily::StreamGenerateContent {
        return None;
    }
    match protocol {
        ProtocolKind::OpenAi => extract_openai_response_usage_from_sse(body),
        _ => None,
    }
}

fn extract_openai_response_usage_from_sse(body: &[u8]) -> Option<UsageSnapshot> {
    let mut cursor = 0usize;
    let mut latest = None;
    while let Some(frame) = next_sse_frame(body, &mut cursor) {
        let Some(data) = parse_sse_data_from_frame(frame) else {
            continue;
        };
        if data == "[DONE]" {
            continue;
        }
        let Ok(event) = serde_json::from_str::<
            gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
        >(&data) else {
            continue;
        };
        if let Some(usage) = usage_from_openai_response_stream_event(&event) {
            latest = Some(usage);
        }
    }
    latest
}

fn usage_from_openai_response_stream_event(
    event: &gproxy_protocol::openai::create_response::stream::ResponseStreamEvent,
) -> Option<UsageSnapshot> {
    use gproxy_protocol::openai::create_response::stream::ResponseStreamEvent;
    match event {
        ResponseStreamEvent::Created { response, .. }
        | ResponseStreamEvent::Queued { response, .. }
        | ResponseStreamEvent::InProgress { response, .. }
        | ResponseStreamEvent::Failed { response, .. }
        | ResponseStreamEvent::Incomplete { response, .. }
        | ResponseStreamEvent::Completed { response, .. } => response
            .usage
            .as_ref()
            .map(usage_from_openai_response_usage),
        _ => None,
    }
}

fn usage_from_openai_response_usage(usage: &ResponseUsage) -> UsageSnapshot {
    UsageSnapshot {
        input_tokens: Some(usage.input_tokens),
        output_tokens: Some(usage.output_tokens),
        total_tokens: Some(usage.total_tokens),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
        cache_read_input_tokens: Some(usage.input_tokens_details.cached_tokens),
        reasoning_tokens: Some(usage.output_tokens_details.reasoning_tokens),
        thoughts_tokens: None,
        tool_use_prompt_tokens: None,
    }
}

fn next_sse_frame<'a>(body: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    if *cursor >= body.len() {
        return None;
    }
    let rest = &body[*cursor..];
    let lf_pos = rest.windows(2).position(|window| window == b"\n\n");
    let crlf_pos = rest.windows(4).position(|window| window == b"\r\n\r\n");
    let (pos, delim_len) = match (lf_pos, crlf_pos) {
        (Some(a), Some(b)) if a <= b => (a, 2),
        (Some(_), Some(b)) => (b, 4),
        (Some(a), None) => (a, 2),
        (None, Some(b)) => (b, 4),
        (None, None) => {
            *cursor = body.len();
            return Some(rest);
        }
    };
    let frame = &rest[..pos];
    *cursor += pos + delim_len;
    Some(frame)
}

fn parse_sse_data_from_frame(frame: &[u8]) -> Option<String> {
    if frame.is_empty() {
        return None;
    }
    let text = std::str::from_utf8(frame).ok()?;
    let mut lines = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            lines.push(value.trim_start().to_string());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

pub(super) fn serialize_claude_model(model: &ClaudeModel) -> Option<String> {
    serde_json::to_value(model)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}

pub(super) fn serialize_openai_embedding_model(model: &OpenAiEmbeddingModel) -> Option<String> {
    serde_json::to_value(model)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn json_pointer_string(value: &serde_json::Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

pub(super) fn extract_model_from_payload(
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: &[u8],
) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    match (operation, protocol) {
        (OperationFamily::ModelList, _) => None,

        (OperationFamily::ModelGet, ProtocolKind::OpenAi) => {
            json_pointer_string(&value, "/path/model")
        }
        (OperationFamily::ModelGet, ProtocolKind::Claude) => {
            json_pointer_string(&value, "/path/model_id")
        }
        (OperationFamily::ModelGet, ProtocolKind::Gemini)
        | (OperationFamily::ModelGet, ProtocolKind::GeminiNDJson) => {
            json_pointer_string(&value, "/path/name")
                .or_else(|| json_pointer_string(&value, "/path/model"))
        }

        (OperationFamily::CountToken, ProtocolKind::OpenAi)
        | (OperationFamily::CountToken, ProtocolKind::Claude) => {
            json_pointer_string(&value, "/model")
                .or_else(|| json_pointer_string(&value, "/body/model"))
        }
        (OperationFamily::CountToken, ProtocolKind::Gemini)
        | (OperationFamily::CountToken, ProtocolKind::GeminiNDJson) => {
            json_pointer_string(&value, "/body/generate_content_request/model")
                .or_else(|| json_pointer_string(&value, "/path/model"))
        }

        (OperationFamily::GenerateContent, ProtocolKind::OpenAi)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAi)
        | (OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::OpenAiChatCompletion)
        | (OperationFamily::GenerateContent, ProtocolKind::Claude)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::Claude)
        | (OperationFamily::Embedding, ProtocolKind::OpenAi)
        | (OperationFamily::Compact, ProtocolKind::OpenAi) => json_pointer_string(&value, "/model")
            .or_else(|| json_pointer_string(&value, "/body/model")),
        (OperationFamily::GenerateContent, ProtocolKind::Gemini)
        | (OperationFamily::GenerateContent, ProtocolKind::GeminiNDJson)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::Gemini)
        | (OperationFamily::StreamGenerateContent, ProtocolKind::GeminiNDJson)
        | (OperationFamily::Embedding, ProtocolKind::Gemini)
        | (OperationFamily::Embedding, ProtocolKind::GeminiNDJson) => {
            json_pointer_string(&value, "/path/model")
        }
        _ => None,
    }
}

pub(super) fn usage_request_context_from_transform_request(
    request: &TransformRequest,
) -> UsageRequestContext {
    UsageRequestContext {
        operation: request.operation(),
        protocol: request.protocol(),
        model: extract_model_from_request(request),
        body_for_estimate: serde_json::to_vec(request).ok(),
    }
}

pub(super) fn usage_request_context_from_payload(
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: &[u8],
) -> UsageRequestContext {
    UsageRequestContext {
        operation,
        protocol,
        model: extract_model_from_payload(operation, protocol, body),
        body_for_estimate: Some(body.to_vec()),
    }
}

pub(super) fn extract_model_from_request(request: &TransformRequest) -> Option<String> {
    match request {
        TransformRequest::ModelListOpenAi(_)
        | TransformRequest::ModelListClaude(_)
        | TransformRequest::ModelListGemini(_) => None,

        TransformRequest::ModelGetOpenAi(value) => Some(value.path.model.clone()),
        TransformRequest::ModelGetClaude(value) => Some(value.path.model_id.clone()),
        TransformRequest::ModelGetGemini(value) => Some(value.path.name.clone()),

        TransformRequest::CountTokenOpenAi(value) => value.body.model.clone(),
        TransformRequest::CountTokenClaude(value) => serialize_claude_model(&value.body.model),
        TransformRequest::CountTokenGemini(value) => {
            if let Some(generate_request) = value.body.generate_content_request.as_ref() {
                Some(generate_request.model.clone())
            } else {
                Some(value.path.model.clone())
            }
        }

        TransformRequest::GenerateContentOpenAiResponse(value)
        | TransformRequest::StreamGenerateContentOpenAiResponse(value) => value.body.model.clone(),

        TransformRequest::GenerateContentOpenAiChatCompletions(value)
        | TransformRequest::StreamGenerateContentOpenAiChatCompletions(value) => {
            Some(value.body.model.clone())
        }

        TransformRequest::GenerateContentClaude(value)
        | TransformRequest::StreamGenerateContentClaude(value) => {
            serialize_claude_model(&value.body.model)
        }

        TransformRequest::GenerateContentGemini(value) => Some(value.path.model.clone()),
        TransformRequest::StreamGenerateContentGeminiSse(value)
        | TransformRequest::StreamGenerateContentGeminiNdjson(value) => {
            Some(value.path.model.clone())
        }

        TransformRequest::EmbeddingOpenAi(value) => {
            serialize_openai_embedding_model(&value.body.model)
        }
        TransformRequest::EmbeddingGemini(value) => Some(value.path.model.clone()),

        TransformRequest::CompactOpenAi(value) => Some(value.body.model.clone()),
    }
}

pub(super) fn normalize_usage_model(model: Option<String>) -> Option<String> {
    model.and_then(|value| {
        let trimmed = value.trim().trim_start_matches('/');
        if trimmed.is_empty() {
            return None;
        }
        let normalized = if let Some(stripped) = trimmed.strip_prefix("models/") {
            stripped.trim()
        } else {
            trimmed
        };
        if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        }
    })
}

pub(super) struct UpstreamAndUsageEventInput<'a> {
    pub(super) auth: RequestAuthContext,
    pub(super) request: &'a UsageRequestContext,
    pub(super) provider_id: Option<i64>,
    pub(super) credential_id: Option<i64>,
    pub(super) request_meta: Option<&'a UpstreamRequestMeta>,
    pub(super) error_status: Option<u16>,
    pub(super) response_status: Option<u16>,
    pub(super) response_headers: &'a [(String, String)],
    pub(super) response_body: Option<Vec<u8>>,
    pub(super) local_response: Option<&'a gproxy_middleware::TransformResponse>,
}

pub(super) async fn enqueue_upstream_and_usage_event(
    state: &AppState,
    input: UpstreamAndUsageEventInput<'_>,
) {
    let UpstreamAndUsageEventInput {
        auth,
        request,
        provider_id,
        credential_id,
        request_meta,
        error_status,
        response_status,
        response_headers,
        response_body,
        local_response,
    } = input;
    let operation = format!("{:?}", request.operation());
    let protocol = format!("{:?}", request.protocol());
    let request_model = normalize_usage_model(request.model.clone());
    let now_unix_ms = now_unix_ms_i64();
    let extracted_usage = local_response.and_then(extract_usage_from_local_response);
    let mask_sensitive_info = state.config.load().global.mask_sensitive_info;
    let persisted_request_body = if mask_sensitive_info {
        None
    } else {
        request_meta.and_then(|meta| meta.body.clone())
    };
    let persisted_response_body = if mask_sensitive_info {
        None
    } else {
        response_body.or_else(|| local_response.and_then(|value| serde_json::to_vec(value).ok()))
    };
    if let Some(meta) = request_meta {
        let upstream_event = UpstreamRequestWrite {
            downstream_trace_id: auth.downstream_trace_id,
            at_unix_ms: now_unix_ms,
            internal: false,
            provider_id,
            credential_id,
            request_method: meta.method.clone(),
            request_headers_json: headers_pairs_to_json(meta.headers.as_slice()),
            request_url: Some(meta.url.clone()),
            request_body: persisted_request_body,
            response_status: response_status.or(error_status).map(i32::from),
            response_headers_json: headers_pairs_to_json(response_headers),
            response_body: persisted_response_body,
        };
        if let Err(err) = state
            .enqueue_storage_write(StorageWriteEvent::UpsertUpstreamRequest(upstream_event))
            .await
        {
            eprintln!("provider: upstream event enqueue failed: {err}");
        }
    }

    if !should_record_usage(request.operation())
        || response_status.map(|value| value >= 400).unwrap_or(true)
    {
        return;
    }
    if request.operation() == OperationFamily::StreamGenerateContent {
        return;
    }

    let mut input_tokens = extracted_usage.and_then(|value| value.input_tokens);
    let mut output_tokens = extracted_usage.and_then(|value| value.output_tokens);
    let cache_read_input_tokens = extracted_usage.and_then(|value| value.cache_read_input_tokens);
    let cache_creation_input_tokens =
        extracted_usage.and_then(|value| value.cache_creation_input_tokens);
    let cache_creation_input_tokens_5min =
        extracted_usage.and_then(|value| value.cache_creation_input_tokens_5min);
    let cache_creation_input_tokens_1h =
        extracted_usage.and_then(|value| value.cache_creation_input_tokens_1h);

    if request.operation() == OperationFamily::Embedding && input_tokens.is_none() {
        input_tokens = estimate_embedding_input_tokens_from_usage_request(state, request).await;
        output_tokens = output_tokens.or(Some(0));
    }
    if request.operation() == OperationFamily::CountToken && input_tokens.is_some() {
        output_tokens = Some(0);
    }

    let usage_event = UsageWrite {
        downstream_trace_id: auth.downstream_trace_id,
        at_unix_ms: now_unix_ms,
        provider_id,
        credential_id,
        user_id: Some(auth.user_id),
        user_key_id: Some(auth.user_key_id),
        operation,
        protocol,
        model: request_model,
        input_tokens,
        output_tokens,
        cache_read_input_tokens,
        cache_creation_input_tokens,
        cache_creation_input_tokens_5min,
        cache_creation_input_tokens_1h,
    };
    if let Err(err) = state
        .enqueue_storage_write(StorageWriteEvent::UpsertUsage(usage_event))
        .await
    {
        eprintln!("provider: usage event enqueue failed: {err}");
    }
}

pub(super) struct UpstreamResponseMeta<'a> {
    pub status: Option<u16>,
    pub headers: &'a [(String, String)],
    pub body: Option<Vec<u8>>,
}

pub(super) async fn enqueue_upstream_request_event_from_meta(
    state: &AppState,
    downstream_trace_id: Option<i64>,
    provider_id: Option<i64>,
    credential_id: Option<i64>,
    request_meta: Option<&UpstreamRequestMeta>,
    response_meta: UpstreamResponseMeta<'_>,
) {
    let Some(meta) = request_meta else {
        return;
    };
    let mask_sensitive_info = state.config.load().global.mask_sensitive_info;
    let upstream_event = UpstreamRequestWrite {
        downstream_trace_id,
        at_unix_ms: now_unix_ms_i64(),
        internal: false,
        provider_id,
        credential_id,
        request_method: meta.method.clone(),
        request_headers_json: headers_pairs_to_json(meta.headers.as_slice()),
        request_url: Some(meta.url.clone()),
        request_body: if mask_sensitive_info {
            None
        } else {
            meta.body.clone()
        },
        response_status: response_meta.status.map(i32::from),
        response_headers_json: headers_pairs_to_json(response_meta.headers),
        response_body: if mask_sensitive_info {
            None
        } else {
            response_meta.body
        },
    };
    if let Err(err) = state
        .enqueue_storage_write(StorageWriteEvent::UpsertUpstreamRequest(upstream_event))
        .await
    {
        eprintln!("provider: upstream event enqueue failed: {err}");
    }
}

pub(super) async fn enqueue_internal_tracked_http_events(
    state: &AppState,
    downstream_trace_id: Option<i64>,
    provider_id: Option<i64>,
    credential_id: Option<i64>,
    events: &[TrackedHttpEvent],
    primary_request_meta: Option<&UpstreamRequestMeta>,
) {
    if events.is_empty() {
        return;
    }
    let mask_sensitive_info = state.config.load().global.mask_sensitive_info;
    for event in events {
        if let Some(primary_meta) = primary_request_meta
            && tracked_http_event_matches_primary_request(event, primary_meta)
        {
            continue;
        }
        let upstream_event = UpstreamRequestWrite {
            downstream_trace_id,
            at_unix_ms: now_unix_ms_i64(),
            internal: true,
            provider_id,
            credential_id,
            request_method: event.request_meta.method.clone(),
            request_headers_json: headers_pairs_to_json(event.request_meta.headers.as_slice()),
            request_url: Some(event.request_meta.url.clone()),
            request_body: if mask_sensitive_info {
                None
            } else {
                event.request_meta.body.clone()
            },
            response_status: event.response_status.map(i32::from),
            response_headers_json: headers_pairs_to_json(event.response_headers.as_slice()),
            response_body: if mask_sensitive_info {
                None
            } else {
                event.response_body.clone()
            },
        };
        if let Err(err) = state
            .enqueue_storage_write(StorageWriteEvent::UpsertUpstreamRequest(upstream_event))
            .await
        {
            eprintln!("provider: tracked http event enqueue failed: {err}");
        }
    }
}

fn tracked_http_event_matches_primary_request(
    event: &TrackedHttpEvent,
    primary_meta: &UpstreamRequestMeta,
) -> bool {
    event.request_meta.method == primary_meta.method
        && event.request_meta.url == primary_meta.url
        && event.request_meta.headers == primary_meta.headers
        && event.request_meta.body == primary_meta.body
}
