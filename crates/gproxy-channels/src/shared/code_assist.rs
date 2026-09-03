use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Value, json};

mod stream;

pub(crate) use stream::decoder;

pub(crate) fn sanitize(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Gemini body JSON: {error}")))?;
    sanitize_value(&mut value)?;
    encode(&value, ChannelError::Prepare)
}

fn sanitize_value(value: &mut Value) -> Result<(), ChannelError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("Gemini body must be an object".into()))?;
    // Code Assist rejects this root option after the Gemini body is nested
    // under `request`; fields named `store` inside parts remain valid data.
    object.remove("store");
    if let Some(config) = object
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
    {
        for name in [
            "maxOutputTokens",
            "max_output_tokens",
            "logprobs",
            "responseLogprobs",
            "response_logprobs",
        ] {
            config.remove(name);
        }
    }
    if let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut)
        && tools.iter().any(|tool| {
            tool.get("functionDeclarations")
                .and_then(Value::as_array)
                .is_some_and(|declarations| !declarations.is_empty())
        })
    {
        tools.retain(|tool| tool.get("functionDeclarations").is_some());
    }
    Ok(())
}

pub(crate) fn wrap(body: &Bytes, model: &str, project: &str) -> Result<Bytes, ChannelError> {
    let mut request: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Gemini body JSON: {error}")))?;
    force_roles(&mut request);
    encode(
        &json!({
            "model":model,
            "project":project,
            "user_prompt_id":prompt_id()?,
            "request":request,
        }),
        ChannelError::Prepare,
    )
}

pub(crate) fn wrap_count(body: &Bytes) -> Result<Bytes, ChannelError> {
    let parsed: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("count body JSON: {error}")))?;
    let mut request = if let Some(request) = parsed.get("generateContentRequest") {
        request.clone()
    } else {
        let mut request = serde_json::Map::new();
        if let Some(contents) = parsed.get("contents") {
            request.insert("contents".into(), contents.clone());
        }
        Value::Object(request)
    };
    sanitize_value(&mut request)?;
    force_roles(&mut request);
    encode(&json!({"request":request}), ChannelError::Prepare)
}

pub(crate) fn unwrap(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Code Assist response JSON: {error}")))?;
    encode(unwrap_value(&value), ChannelError::Observe)
}

pub(crate) fn unwrap_value(value: &Value) -> &Value {
    value.get("response").unwrap_or(value)
}

fn force_roles(request: &mut Value) {
    if let Some(contents) = request.get_mut("contents").and_then(Value::as_array_mut) {
        for content in contents {
            if let Some(content) = content.as_object_mut() {
                content
                    .entry("role")
                    .or_insert_with(|| Value::String("user".into()));
            }
        }
    }
}

fn prompt_id() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| ChannelError::Prepare("Code Assist prompt id randomness failed".into()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn encode(
    value: &Value,
    error: impl FnOnce(String) -> ChannelError,
) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(|cause| error(cause.to_string()))
}
