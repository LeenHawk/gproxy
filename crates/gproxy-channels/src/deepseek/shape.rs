use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Map, Value};

const MAX_OUTPUT_TOKENS: u64 = 384_000;
const UNSUPPORTED: &[&str] = &[
    "audio",
    "function_call",
    "functions",
    "logit_bias",
    "max_completion_tokens",
    "metadata",
    "modalities",
    "n",
    "parallel_tool_calls",
    "prediction",
    "prompt_cache_key",
    "prompt_cache_retention",
    "safety_identifier",
    "seed",
    "service_tier",
    "store",
    "user",
    "verbosity",
    "web_search_options",
];

pub(super) fn request(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body).map_err(prepare_error)?;
    let root = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("DeepSeek Chat body must be an object".into()))?;
    normalize_extra_body(root);
    normalize_tokens(root);
    for field in UNSUPPORTED {
        root.remove(*field);
    }
    normalize_roles(root);
    super::tools::normalize(root);
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(prepare_error)
}

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body).map_err(observe_error)?;
    if normalize_response_value(&mut value) {
        serde_json::to_vec(&value)
            .map(Bytes::from)
            .map_err(observe_error)
    } else {
        Ok(body.clone())
    }
}

pub(super) fn normalize_response_value(value: &mut Value) -> bool {
    let Some(root) = value.as_object_mut() else {
        return false;
    };
    let mut changed = false;
    if let Some(choices) = root.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices {
            if let Some(reason) = choice
                .as_object_mut()
                .and_then(|choice| choice.get_mut("finish_reason"))
                && reason.as_str() == Some("insufficient_system_resource")
            {
                *reason = Value::String("length".into());
                changed = true;
            }
        }
    }
    if let Some(usage) = root.get_mut("usage").and_then(Value::as_object_mut)
        && let Some(cached) = usage.get("prompt_cache_hit_tokens").and_then(Value::as_u64)
    {
        let details = usage
            .entry("prompt_tokens_details")
            .or_insert_with(|| Value::Object(Map::new()));
        if !details.is_object() {
            *details = Value::Object(Map::new());
        }
        details
            .as_object_mut()
            .expect("details was normalized to an object")
            .entry("cached_tokens")
            .or_insert_with(|| Value::from(cached));
        changed = true;
    }
    changed
}

fn normalize_tokens(root: &mut Map<String, Value>) {
    if let Some(value) = root.get("max_tokens").and_then(Value::as_u64) {
        root.insert(
            "max_tokens".into(),
            Value::from(value.min(MAX_OUTPUT_TOKENS)),
        );
    }
    if let Some(value) = root.get("max_completion_tokens").and_then(Value::as_u64) {
        root.insert(
            "max_completion_tokens".into(),
            Value::from(value.min(MAX_OUTPUT_TOKENS)),
        );
    }
    if !root.contains_key("max_tokens")
        && let Some(value) = root.remove("max_completion_tokens")
    {
        root.insert("max_tokens".into(), value);
    }
}

fn normalize_extra_body(root: &mut Map<String, Value>) {
    let Some(extra) = root.remove("extra_body") else {
        return;
    };
    if !root.contains_key("thinking")
        && let Some(thinking) = find_thinking(&extra)
    {
        root.insert("thinking".into(), thinking);
    }
}

fn find_thinking(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    if let Some(kind) = object
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
    {
        let normalized = match kind {
            "adaptive" | "enabled" => Some(serde_json::json!({"type":"enabled"})),
            "disabled" => Some(serde_json::json!({"type":"disabled"})),
            _ => None,
        };
        if normalized.is_some() {
            return normalized;
        }
    }
    object.get("extra_body").and_then(find_thinking)
}

fn normalize_roles(root: &mut Map<String, Value>) {
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        if message.get("role").and_then(Value::as_str) == Some("developer") {
            message["role"] = Value::String("system".into());
        }
    }
}

fn prepare_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("DeepSeek Chat JSON: {error}"))
}

fn observe_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(format!("DeepSeek Chat JSON: {error}"))
}
