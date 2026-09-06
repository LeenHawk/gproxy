use bytes::Bytes;
use gproxy_protocol::{Operation, OperationKey, WireFamily};
use http::{HeaderMap, HeaderValue, header};
use serde_json::{Value, json};

use crate::boundary::{ExecOutcome, ResponseBody};
use crate::error::CoreError;

const DEFAULT_INSTRUCTIONS: &str = "You are Codex, a coding agent. Work with the user in the current workspace, follow repository instructions, and complete the requested task carefully.";

pub(super) fn render(
    headers: &HeaderMap,
    key: OperationKey,
    mut outcome: ExecOutcome,
) -> Result<ExecOutcome, CoreError> {
    if key != OperationKey::family(Operation::ListModels, WireFamily::OpenAi)
        || !outcome.status.is_success()
    {
        return Ok(outcome);
    }
    outcome.headers.append(
        header::VARY,
        HeaderValue::from_static("User-Agent, originator"),
    );
    if !is_codex(headers) {
        return Ok(outcome);
    }
    let ResponseBody::Full(body) = &outcome.body else {
        return Err(CoreError::Transform("model list must be buffered".into()));
    };
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| CoreError::Transform(format!("model list JSON: {error}")))?;
    if value.get("models").is_some_and(Value::is_array) {
        return Ok(outcome);
    }
    let models = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Transform("model list is missing data".into()))?
        .iter()
        .map(model)
        .collect::<Result<Vec<_>, _>>()?;
    outcome.body = ResponseBody::Full(Bytes::from(
        serde_json::to_vec(&json!({ "models": models })).expect("model list JSON serializes"),
    ));
    outcome.headers.remove(header::CONTENT_LENGTH);
    outcome.headers.remove(header::ETAG);
    Ok(outcome)
}

fn is_codex(headers: &HeaderMap) -> bool {
    ["originator", "user-agent"]
        .into_iter()
        .filter_map(|name| headers.get(name)?.to_str().ok())
        .flat_map(str::split_ascii_whitespace)
        .any(|product| {
            let name = product
                .split('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            name == "codex" || name.starts_with("codex_") || name.starts_with("codex-")
        })
}

fn model(source: &Value) -> Result<Value, CoreError> {
    let id = source
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::Transform("model entry is missing id".into()))?;
    let mut target = json!({
        "slug": id,
        "display_name": source.get("display_name").and_then(Value::as_str).unwrap_or(id),
        "description": null,
        "supported_reasoning_levels": [],
        "shell_type": "default",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 0,
        "base_instructions": source
            .get("instructions")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_INSTRUCTIONS),
        "support_verbosity": false,
        "supports_reasoning_summary_parameter": false,
        "truncation_policy": { "mode": "bytes", "limit": 10_000 },
        "context_window": source.get("context_window").or_else(|| source.get("context_length")),
        "experimental_supported_tools": [],
        "input_modalities": ["text"],
    });
    for field in [
        "description",
        "default_reasoning_level",
        "supported_reasoning_levels",
        "service_tiers",
        "default_service_tier",
        "shell_type",
        "base_instructions",
        "model_messages",
        "support_verbosity",
        "default_verbosity",
        "supports_reasoning_summary_parameter",
        "default_reasoning_summary",
        "apply_patch_tool_type",
        "truncation_policy",
        "max_context_window",
        "auto_compact_token_limit",
        "effective_context_window_percent",
        "experimental_supported_tools",
        "input_modalities",
        "supports_image_detail_original",
        "supports_search_tool",
    ] {
        if let Some(value) = source.get(field).filter(|value| !value.is_null()) {
            target[field] = value.clone();
        }
    }
    Ok(target)
}
