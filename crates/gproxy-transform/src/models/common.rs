use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn claude_to_openai(model: claude::ModelInfo) -> Result<openai::Model, TransformError> {
    let thinking_supported = model
        .capabilities
        .as_ref()
        .map(|capabilities| capabilities.thinking.supported);
    Ok(crate::wire!(openai::Model {
        id: wire_string(&model.id)?.into(),
        created: None,
        display_name: model.display_name,
        description: None,
        instructions: None,
        context_window: model.max_input_tokens,
        max_context_window: None,
        max_output_tokens: model.max_tokens,
        thinking_supported,
        input_modalities: model.capabilities.as_ref().map(|capabilities| {
            let mut values = vec!["text".into()];
            if capabilities.image_input.supported {
                values.push("image".into());
            }
            if capabilities.pdf_input.supported {
                values.push("pdf".into());
            }
            values
        }),
        output_modalities: None,
        supported_parameters: model.capabilities.as_ref().map(supported_parameters),
        supported_reasoning_levels: model.capabilities.as_ref().map(reasoning_levels),
        default_reasoning_level: None,
        service_tiers: None,
        default_service_tier: None,
        generation_methods: None,
        supported_actions: None,
        object: openai::ModelObjectType::Model,
        owned_by: Some("unknown".into()),
        rest: Default::default(),
    }))
}

pub(crate) fn openai_to_claude(model: openai::Model) -> Result<claude::ModelInfo, TransformError> {
    let id = wire_string(&model.id)?;
    let capabilities = capabilities(&model);
    Ok(crate::wire!(claude::ModelInfo {
        id: id.clone().into(),
        allowed_fallback_models: None,
        type_: claude::ModelObjectType::Known(claude::ModelObjectTypeKnown::Model),
        created_at: Some("1970-01-01T00:00:00Z".into()),
        display_name: model.display_name.or(Some(id)),
        max_input_tokens: model.context_window.or(model.max_context_window),
        max_tokens: model.max_output_tokens,
        capabilities,
        rest: Default::default(),
    }))
}

fn capabilities(model: &openai::Model) -> Option<claude::ModelCapabilities> {
    let parameters = model.supported_parameters.as_deref().unwrap_or_default();
    let modalities = model.input_modalities.as_deref().unwrap_or_default();
    if model.thinking_supported.is_none()
        && model.supported_parameters.is_none()
        && model.input_modalities.is_none()
        && model.supported_reasoning_levels.is_none()
    {
        return None;
    }
    let has = |name: &str| parameters.iter().any(|value| value == name);
    let support = |supported| {
        crate::wire!(claude::CapabilitySupport {
            supported,
            rest: Default::default(),
        })
    };
    let effort = |name: &str| {
        model
            .supported_reasoning_levels
            .as_ref()
            .is_some_and(|values| values.iter().any(|value| value.effort == name))
            .then(|| support(true))
    };
    Some(crate::wire!(claude::ModelCapabilities {
        batch: support(has("batch")),
        citations: support(has("citations")),
        code_execution: support(has("code_execution")),
        context_management: claude::ContextManagementCapability {
            supported: has("context_management"),
            clear_thinking_20251015: None,
            clear_tool_uses_20250919: None,
            compact_20260112: None,
            rest: Default::default(),
        },
        effort: claude::EffortCapability {
            supported: model
                .supported_reasoning_levels
                .as_ref()
                .is_some_and(|v| !v.is_empty()),
            low: effort("low"),
            medium: effort("medium"),
            high: effort("high"),
            xhigh: effort("xhigh"),
            max: effort("max"),
            rest: Default::default(),
        },
        image_input: support(modalities.iter().any(|value| value == "image")),
        pdf_input: support(
            modalities
                .iter()
                .any(|value| value == "pdf" || value == "file")
        ),
        structured_outputs: support(has("structured_outputs")),
        thinking: claude::ThinkingCapability {
            supported: model.thinking_supported.unwrap_or(false),
            types: claude::ThinkingTypes {
                adaptive: None,
                enabled: None,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
        rest: Default::default(),
    }))
}

fn supported_parameters(capabilities: &claude::ModelCapabilities) -> Vec<String> {
    [
        (capabilities.batch.supported, "batch"),
        (capabilities.citations.supported, "citations"),
        (capabilities.code_execution.supported, "code_execution"),
        (
            capabilities.structured_outputs.supported,
            "structured_outputs",
        ),
        (capabilities.thinking.supported, "reasoning"),
    ]
    .into_iter()
    .filter(|(supported, _)| *supported)
    .map(|(_, name)| name.into())
    .collect()
}

fn reasoning_levels(capabilities: &claude::ModelCapabilities) -> Vec<openai::ModelReasoningLevel> {
    [
        (capabilities.effort.low.as_ref(), "low"),
        (capabilities.effort.medium.as_ref(), "medium"),
        (capabilities.effort.high.as_ref(), "high"),
        (capabilities.effort.xhigh.as_ref(), "xhigh"),
        (capabilities.effort.max.as_ref(), "max"),
    ]
    .into_iter()
    .filter(|(support, _)| support.is_some_and(|value| value.supported))
    .map(|(_, effort)| {
        crate::wire!(openai::ModelReasoningLevel {
            effort: effort.into(),
            description: String::new(),
        })
    })
    .collect()
}

pub(crate) fn wire_string<T: serde::Serialize>(value: &T) -> Result<String, TransformError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| TransformError::shape("model id", "expected a string"))
}
