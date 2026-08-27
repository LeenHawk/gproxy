use gproxy_protocol::WireFamily;
use serde_json::{Value, json};

use crate::control::ExposedModel;

pub(super) fn render_list(family: WireFamily, models: Vec<ExposedModel>) -> Value {
    let entries = models
        .iter()
        .map(|model| render_model(family, model))
        .collect::<Vec<_>>();
    match family {
        WireFamily::OpenAi => json!({ "object": "list", "data": entries }),
        WireFamily::Claude => json!({
            "data": entries,
            "first_id": models.first().map(|model| model.id.as_str()),
            "last_id": models.last().map(|model| model.id.as_str()),
            "has_more": false,
        }),
        WireFamily::Gemini => json!({ "models": entries }),
    }
}

pub(super) fn render_model(family: WireFamily, model: &ExposedModel) -> Value {
    match family {
        WireFamily::OpenAi => openai(model),
        WireFamily::Claude => claude(model),
        WireFamily::Gemini => gemini(model),
    }
}

fn openai(model: &ExposedModel) -> Value {
    let mut value = json!({
        "id": model.id,
        "object": "model",
        "created": 0,
        "owned_by": "GPROXY",
    });
    if let Some(limit) = model.context_window {
        value["context_length"] = json!(limit);
        value["context_window"] = json!(limit);
    }
    if let Some(limit) = model.max_output_tokens {
        value["max_completion_tokens"] = json!(limit);
    }
    if model.thinking_supported == Some(true) {
        value["supported_parameters"] = json!(["reasoning"]);
    }
    value
}

fn claude(model: &ExposedModel) -> Value {
    let mut value = json!({
        "id": model.id,
        "type": "model",
        "display_name": model.display_name.as_deref().unwrap_or(&model.id),
        "created_at": "1970-01-01T00:00:00Z",
    });
    if let Some(limit) = model.context_window {
        value["max_input_tokens"] = json!(limit);
    }
    if let Some(limit) = model.max_output_tokens {
        value["max_tokens"] = json!(limit);
    }
    if model.thinking_supported.is_some()
        || model.thinking_adaptive_supported.is_some()
        || model.thinking_enabled_supported.is_some()
    {
        let mut thinking = json!({});
        if let Some(supported) = model.thinking_supported {
            thinking["supported"] = json!(supported);
        }
        let mut types = json!({});
        if let Some(supported) = model.thinking_adaptive_supported {
            types["adaptive"] = json!({ "supported": supported });
        }
        if let Some(supported) = model.thinking_enabled_supported {
            types["enabled"] = json!({ "supported": supported });
        }
        thinking["types"] = types;
        value["capabilities"] = json!({ "thinking": thinking });
    }
    value
}

fn gemini(model: &ExposedModel) -> Value {
    let mut value = json!({ "name": format!("models/{}", model.id) });
    if let Some(display_name) = &model.display_name {
        value["displayName"] = json!(display_name);
    }
    if let Some(limit) = model.context_window {
        value["inputTokenLimit"] = json!(limit);
    }
    if let Some(limit) = model.max_output_tokens {
        value["outputTokenLimit"] = json!(limit);
    }
    if let Some(supported) = model.thinking_supported {
        value["thinking"] = json!(supported);
    }
    value
}
