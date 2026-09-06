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
    if let Some(display_name) = &model.display_name {
        value["display_name"] = json!(display_name);
    }
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
    let metadata = &model.metadata;
    for (name, field) in [
        (
            "description",
            metadata.description.as_ref().map(|value| json!(value)),
        ),
        (
            "instructions",
            metadata.instructions.as_ref().map(|value| json!(value)),
        ),
        (
            "max_context_window",
            metadata.max_context_window.map(|value| json!(value)),
        ),
        (
            "input_modalities",
            metadata.input_modalities.as_ref().map(|value| json!(value)),
        ),
        (
            "output_modalities",
            metadata
                .output_modalities
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "supported_parameters",
            metadata
                .supported_parameters
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "supported_reasoning_levels",
            metadata.reasoning_levels.as_ref().map(|value| json!(value)),
        ),
        (
            "default_reasoning_level",
            metadata
                .default_reasoning_level
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "service_tiers",
            metadata.service_tiers.as_ref().map(|value| json!(value)),
        ),
        (
            "default_service_tier",
            metadata
                .default_service_tier
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "shell_type",
            metadata.shell_type.as_ref().map(|value| json!(value)),
        ),
        (
            "support_verbosity",
            metadata.support_verbosity.map(|value| json!(value)),
        ),
        (
            "default_verbosity",
            metadata
                .default_verbosity
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "supports_reasoning_summary_parameter",
            metadata
                .supports_reasoning_summary_parameter
                .map(|value| json!(value)),
        ),
        (
            "default_reasoning_summary",
            metadata
                .default_reasoning_summary
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "apply_patch_tool_type",
            metadata
                .apply_patch_tool_type
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "web_search_tool_type",
            metadata
                .web_search_tool_type
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "auto_compact_token_limit",
            metadata.auto_compact_token_limit.map(|value| json!(value)),
        ),
        (
            "effective_context_window_percent",
            metadata
                .effective_context_window_percent
                .map(|value| json!(value)),
        ),
        (
            "supports_image_detail_original",
            metadata
                .supports_image_detail_original
                .map(|value| json!(value)),
        ),
        (
            "supports_search_tool",
            metadata.supports_search_tool.map(|value| json!(value)),
        ),
    ] {
        if let Some(field) = field {
            value[name] = field;
        }
    }
    if let (Some(mode), Some(limit)) = (&metadata.truncation_mode, metadata.truncation_limit) {
        value["truncation_policy"] = json!({ "mode": mode, "limit": limit });
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
        || model.metadata.batch_supported.is_some()
        || model.metadata.citations_supported.is_some()
        || model.metadata.code_execution_supported.is_some()
        || model.metadata.context_management_supported.is_some()
        || model.metadata.structured_outputs_supported.is_some()
    {
        let supported = |value: Option<bool>| json!({ "supported": value.unwrap_or(false) });
        let mut thinking = json!({ "supported": model.thinking_supported.unwrap_or(false) });
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
        let effort = model
            .metadata
            .reasoning_levels
            .as_ref()
            .map(|levels| {
                let mut value = json!({ "supported": !levels.is_empty() });
                for level in levels {
                    if matches!(
                        level.effort.as_str(),
                        "low" | "medium" | "high" | "xhigh" | "max"
                    ) {
                        value[&level.effort] = supported(Some(true));
                    }
                }
                value
            })
            .unwrap_or_else(|| json!({ "supported": false }));
        value["capabilities"] = json!({
            "batch": supported(model.metadata.batch_supported),
            "citations": supported(model.metadata.citations_supported),
            "code_execution": supported(model.metadata.code_execution_supported),
            "context_management": { "supported": model.metadata.context_management_supported.unwrap_or(false) },
            "effort": effort,
            "image_input": supported(model.metadata.input_modalities.as_ref().map(|values| values.iter().any(|value| value == "image"))),
            "pdf_input": supported(model.metadata.pdf_input_supported),
            "structured_outputs": supported(model.metadata.structured_outputs_supported),
            "thinking": thinking,
        });
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
    if let Some(description) = &model.metadata.description {
        value["description"] = json!(description);
    }
    if let Some(methods) = &model.metadata.generation_methods {
        value["supportedGenerationMethods"] = json!(methods);
    }
    if let Some(actions) = &model.metadata.supported_actions {
        value["supportedActions"] = json!(actions);
    }
    value
}
