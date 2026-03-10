use super::*;

pub(super) fn transform_openai_ws_request_to_stream(
    request: TransformRequest,
) -> Result<TransformRequest, UpstreamError> {
    gproxy_middleware::transform_request(
        request,
        TransformRoute {
            src_operation: OperationFamily::OpenAiResponseWebSocket,
            src_protocol: ProtocolKind::OpenAi,
            dst_operation: OperationFamily::StreamGenerateContent,
            dst_protocol: ProtocolKind::OpenAi,
        },
    )
    .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))
}

pub(super) async fn try_local_codex_count_token_response(
    request: &TransformRequest,
    http_client: &WreqClient,
    token_resolution: TokenizerResolutionContext<'_>,
) -> Result<Option<TransformResponse>, UpstreamError> {
    let TransformRequest::CountTokenOpenAi(value) = request else {
        return Ok(None);
    };

    let input_tokens = count_openai_input_tokens_with_resolution(
        token_resolution.tokenizer_store,
        http_client,
        token_resolution.hf_token,
        token_resolution.hf_url,
        value.body.model.as_deref(),
        &value.body,
    )
    .await?;

    let response_json = json!({
        "stats_code": 200,
        "headers": {},
        "body": {
            "input_tokens": input_tokens,
            "object": "response.input_tokens",
        }
    });
    let response = serde_json::from_value(response_json)
        .map_err(|err| UpstreamError::SerializeRequest(err.to_string()))?;
    Ok(Some(TransformResponse::CountTokenOpenAi(response)))
}

pub(super) fn codex_models_path() -> String {
    format!("/models?client_version={CLIENT_VERSION}")
}

pub(super) fn normalize_model_id(model: &str) -> String {
    let model = model.trim().trim_start_matches('/');
    let model = model.strip_prefix("models/").unwrap_or(model);
    model.strip_prefix("codex/").unwrap_or(model).to_string()
}

pub(super) fn normalize_codex_response_request_body(body: &mut Value, is_stream: bool) {
    let Some(map) = body.as_object_mut() else {
        return;
    };

    if let Some(model) = map.get_mut("model")
        && let Some(model_str) = model.as_str()
    {
        *model = Value::String(normalize_model_id(model_str));
    }

    map.insert("store".to_string(), Value::Bool(false));
    map.remove("max_output_tokens");
    map.remove("metadata");
    map.remove("stream_options");
    map.remove("temperature");
    map.remove("top_p");
    map.remove("top_logprobs");
    map.remove("safety_identifier");
    map.remove("truncation");
    extract_codex_instructions_from_input_messages(map);

    if is_stream {
        map.insert("stream".to_string(), Value::Bool(true));
    } else {
        map.insert("stream".to_string(), Value::Bool(false));
    }

    if map
        .get("instructions")
        .is_some_and(|value| !value.is_string())
    {
        map.insert("instructions".to_string(), Value::String(String::new()));
    }

    if !map.contains_key("instructions") {
        map.insert("instructions".to_string(), Value::String(String::new()));
    }

    if let Some(input) = map.get("input")
        && let Some(text) = input.as_str()
    {
        map.insert(
            "input".to_string(),
            json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": text,
                }
            ]),
        );
    }
}

pub(super) fn apply_codex_priority_tier_override(
    body: Option<&[u8]>,
    priority_tier: Option<bool>,
) -> Option<Vec<u8>> {
    let Some(body) = body else {
        return None;
    };
    let Some(true) = priority_tier else {
        return Some(body.to_vec());
    };

    let Ok(mut body_json) = serde_json::from_slice::<Value>(body) else {
        return Some(body.to_vec());
    };
    let Some(map) = body_json.as_object_mut() else {
        return Some(body.to_vec());
    };

    map.insert(
        "service_tier".to_string(),
        Value::String("priority".to_string()),
    );

    serde_json::to_vec(&body_json)
        .ok()
        .or_else(|| Some(body.to_vec()))
}

pub(super) fn ensure_codex_session_id_header(
    extra_headers: &mut Vec<(String, String)>,
    body: Option<&[u8]>,
) {
    let session_id = extra_headers
        .iter()
        .find(|(name, _)| is_session_id_header(name))
        .and_then(|(_, value)| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
        .or_else(|| synthesize_codex_session_id(body));

    let Some(session_id) = session_id else {
        return;
    };

    extra_headers.retain(|(name, _)| !is_session_id_header(name));
    extra_headers.push((SESSION_ID_HEADER.to_string(), session_id));
}

pub(super) fn is_session_id_header(name: &str) -> bool {
    name.eq_ignore_ascii_case(SESSION_ID_HEADER) || name.eq_ignore_ascii_case(SESSION_ID_ALT_HEADER)
}

pub(super) fn synthesize_codex_session_id(body: Option<&[u8]>) -> Option<String> {
    let body_json = serde_json::from_slice::<Value>(body?).ok()?;
    let session_marker = codex_session_marker_from_body(&body_json)
        .or_else(|| codex_initial_prompt_session_marker(&body_json))?;
    Some(stable_codex_session_id(session_marker.as_str()))
}

pub(super) fn codex_session_marker_from_body(body_json: &Value) -> Option<String> {
    body_json
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let conversation = body_json.get("conversation")?;
            match conversation {
                Value::String(value) => {
                    let value = value.trim();
                    (!value.is_empty()).then(|| value.to_string())
                }
                Value::Object(value) => value
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(ToOwned::to_owned),
                _ => None,
            }
        })
        .or_else(|| {
            body_json
                .get("previous_response_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

pub(super) fn codex_initial_prompt_session_marker(body_json: &Value) -> Option<String> {
    let mut marker = serde_json::Map::new();

    if let Some(instructions) = body_json
        .get("instructions")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        marker.insert(
            "instructions".to_string(),
            Value::String(instructions.to_string()),
        );
    }

    if let Some(first_input) = codex_first_input_session_marker(body_json.get("input")) {
        marker.insert("input".to_string(), first_input);
    }

    (!marker.is_empty())
        .then(|| serde_json::to_string(&Value::Object(marker)).ok())
        .flatten()
}

pub(super) fn codex_first_input_session_marker(input: Option<&Value>) -> Option<Value> {
    match input? {
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then(|| Value::String(text.to_string()))
        }
        Value::Array(items) => items.first().cloned(),
        Value::Null => None,
        value => Some(value.clone()),
    }
}

pub(super) fn stable_codex_session_id(marker: &str) -> String {
    let digest = Sha256::digest(format!("gproxy.codex.session:{marker}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

pub(super) fn extract_codex_instructions_from_input_messages(
    map: &mut serde_json::Map<String, Value>,
) {
    let mut extracted = Vec::new();

    if let Some(Value::Array(items)) = map.get_mut("input") {
        let source_items = std::mem::take(items);
        let mut kept = Vec::with_capacity(source_items.len());
        for item in source_items {
            let role = item.get("role").and_then(Value::as_str);
            if matches!(role, Some("system" | "developer")) {
                if let Some(text) = extract_codex_message_text(item.get("content")) {
                    extracted.push(text);
                }
                continue;
            }
            kept.push(item);
        }
        *items = kept;
    }

    if extracted.is_empty() {
        return;
    }

    let extracted_text = extracted.join("\n\n");
    let current = map
        .get("instructions")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let merged = match current {
        Some(base) => format!("{base}\n\n{extracted_text}"),
        None => extracted_text,
    };
    map.insert("instructions".to_string(), Value::String(merged));
}

pub(super) fn extract_codex_message_text(content: Option<&Value>) -> Option<String> {
    let content = content?;
    match content {
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        Value::Array(parts) => {
            let mut out = Vec::new();
            for part in parts {
                if let Some(text) = extract_codex_text_part(part) {
                    out.push(text);
                }
            }
            (!out.is_empty()).then(|| out.join("\n"))
        }
        Value::Object(_) => extract_codex_text_part(content),
        _ => None,
    }
}

pub(super) fn extract_codex_text_part(part: &Value) -> Option<String> {
    match part {
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        Value::Object(obj) => {
            let text = obj
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| obj.get("refusal").and_then(Value::as_str))?;
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        _ => None,
    }
}

pub(super) fn normalize_codex_compact_request_body(body: &mut Value) {
    let Some(map) = body.as_object_mut() else {
        return;
    };

    if let Some(model) = map.get_mut("model")
        && let Some(model_str) = model.as_str()
    {
        *model = Value::String(normalize_model_id(model_str));
    }

    if let Some(input) = map.get("input")
        && let Some(text) = input.as_str()
    {
        map.insert(
            "input".to_string(),
            json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": text,
                }
            ]),
        );
    }
}

pub(super) fn build_model_list_local_response(status_code: u16, bytes: &[u8]) -> TransformResponse {
    if status_code == 200 {
        let parsed = serde_json::from_slice::<Value>(bytes).ok();
        if let Some(parsed) = parsed
            && let Some(body) = normalize_openai_model_list_value(&parsed)
        {
            let response_json = json!({
                "stats_code": 200,
                "headers": {},
                "body": body,
            });
            if let Ok(response) = serde_json::from_value(response_json) {
                return TransformResponse::ModelListOpenAi(response);
            }
        }

        return model_list_error_response(502, "invalid codex model-list payload");
    }

    let message = extract_upstream_error_message(bytes)
        .unwrap_or_else(|| format!("upstream status {status_code}"));
    model_list_error_response(status_code, &message)
}

pub(super) fn build_model_get_local_response(
    status_code: u16,
    bytes: &[u8],
    target: &str,
) -> TransformResponse {
    if status_code == 200 {
        let parsed = serde_json::from_slice::<Value>(bytes).ok();
        if let Some(parsed) = parsed
            && let Some(list_value) = normalize_openai_model_list_value(&parsed)
            && let Some(model) = find_model_in_openai_list(&list_value, target)
        {
            let response_json = json!({
                "stats_code": 200,
                "headers": {},
                "body": model,
            });
            if let Ok(response) = serde_json::from_value(response_json) {
                return TransformResponse::ModelGetOpenAi(response);
            }
        }

        let message = format!("model {target} not found");
        return model_get_error_response(404, &message);
    }

    let message = extract_upstream_error_message(bytes)
        .unwrap_or_else(|| format!("upstream status {status_code}"));
    model_get_error_response(status_code, &message)
}

pub(super) fn normalize_openai_model_list_value(value: &Value) -> Option<Value> {
    if is_openai_model_list(value) {
        return Some(value.clone());
    }

    let models = value.get("models")?.as_array()?;
    let mut data = Vec::new();
    for item in models {
        if let Some(model) = normalize_openai_model_value(item) {
            data.push(model);
        }
    }

    Some(json!({
        "object": "list",
        "data": data,
    }))
}

pub(super) fn normalize_openai_model_value(value: &Value) -> Option<Value> {
    if is_openai_model_value(value) {
        return Some(value.clone());
    }

    let object = value.as_object()?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| object.get("slug").and_then(Value::as_str))?;

    let created = object
        .get("created")
        .and_then(Value::as_u64)
        .unwrap_or_else(current_unix_ts);
    let owned_by = object
        .get("owned_by")
        .and_then(Value::as_str)
        .unwrap_or("openai");

    Some(json!({
        "id": normalize_model_id(id),
        "object": "model",
        "owned_by": owned_by,
        "created": created,
    }))
}

pub(super) fn is_openai_model_list(value: &Value) -> bool {
    value
        .get("object")
        .and_then(Value::as_str)
        .map(|object| object == "list")
        .unwrap_or(false)
        && value.get("data").and_then(Value::as_array).is_some()
}

pub(super) fn is_openai_model_value(value: &Value) -> bool {
    value
        .get("object")
        .and_then(Value::as_str)
        .map(|object| object == "model")
        .unwrap_or(false)
        && value.get("id").and_then(Value::as_str).is_some()
        && value.get("owned_by").and_then(Value::as_str).is_some()
        && value.get("created").and_then(Value::as_u64).is_some()
}

pub(super) fn find_model_in_openai_list(list: &Value, target: &str) -> Option<Value> {
    let data = list.get("data")?.as_array()?;
    data.iter()
        .find(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(|id| normalize_model_id(id) == target)
                .unwrap_or(false)
        })
        .cloned()
}

pub(super) fn extract_upstream_error_message(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        return Some(message.to_string());
    }
    if let Some(message) = value.get("error").and_then(Value::as_str) {
        return Some(message.to_string());
    }
    value
        .get("message")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub(super) fn model_list_error_response(status_code: u16, message: &str) -> TransformResponse {
    let response_json = json!({
        "stats_code": status_code,
        "headers": {},
        "body": {
            "error": {
                "message": message,
                "type": "invalid_request_error",
                "param": null,
                "code": "upstream_error",
            }
        }
    });

    match serde_json::from_value(response_json) {
        Ok(response) => TransformResponse::ModelListOpenAi(response),
        Err(_) => internal_model_list_fallback(),
    }
}

pub(super) fn model_get_error_response(status_code: u16, message: &str) -> TransformResponse {
    let response_json = json!({
        "stats_code": status_code,
        "headers": {},
        "body": {
            "error": {
                "message": message,
                "type": "invalid_request_error",
                "param": "model",
                "code": "upstream_error",
            }
        }
    });

    match serde_json::from_value(response_json) {
        Ok(response) => TransformResponse::ModelGetOpenAi(response),
        Err(_) => internal_model_get_fallback(),
    }
}

pub(super) fn internal_model_list_fallback() -> TransformResponse {
    let response_json = json!({
        "stats_code": 500,
        "headers": {},
        "body": {
            "error": {
                "message": "internal serialization error",
                "type": "server_error",
                "param": null,
                "code": "internal_error",
            }
        }
    });
    let response = serde_json::from_value(response_json)
        .expect("internal fallback model list response must be valid");
    TransformResponse::ModelListOpenAi(response)
}

pub(super) fn internal_model_get_fallback() -> TransformResponse {
    let response_json = json!({
        "stats_code": 500,
        "headers": {},
        "body": {
            "error": {
                "message": "internal serialization error",
                "type": "server_error",
                "param": "model",
                "code": "internal_error",
            }
        }
    });
    let response = serde_json::from_value(response_json)
        .expect("internal fallback model get response must be valid");
    TransformResponse::ModelGetOpenAi(response)
}

pub(super) fn current_unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(super) fn codex_credential_update(
    credential_id: i64,
    refreshed: &CodexRefreshedToken,
) -> UpstreamCredentialUpdate {
    UpstreamCredentialUpdate::CodexTokenRefresh {
        credential_id,
        access_token: refreshed.access_token.clone(),
        refresh_token: refreshed.refresh_token.clone(),
        expires_at_unix_ms: refreshed.expires_at_unix_ms,
        user_email: refreshed.user_email.clone(),
        id_token: refreshed.id_token.clone(),
    }
}
