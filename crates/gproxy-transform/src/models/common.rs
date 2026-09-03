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
        context_window: model.max_input_tokens,
        max_context_window: None,
        max_output_tokens: model.max_tokens,
        thinking_supported,
        object: openai::ModelObjectType::Model,
        owned_by: Some("unknown".into()),
        rest: Default::default(),
    }))
}

pub(crate) fn openai_to_claude(model: openai::Model) -> Result<claude::ModelInfo, TransformError> {
    let id = wire_string(&model.id)?;
    Ok(crate::wire!(claude::ModelInfo {
        id: id.clone().into(),
        allowed_fallback_models: None,
        type_: claude::ModelObjectType::Known(claude::ModelObjectTypeKnown::Model),
        created_at: Some("1970-01-01T00:00:00Z".into()),
        display_name: model.display_name.or(Some(id)),
        max_input_tokens: model.context_window.or(model.max_context_window),
        max_tokens: model.max_output_tokens,
        capabilities: model.thinking_supported.map(capabilities),
        rest: Default::default(),
    }))
}

fn capabilities(supported: bool) -> claude::ModelCapabilities {
    let support = || {
        crate::wire!(claude::CapabilitySupport {
            supported: false,
            rest: Default::default(),
        })
    };
    crate::wire!(claude::ModelCapabilities {
        batch: support(),
        citations: support(),
        code_execution: support(),
        context_management: claude::ContextManagementCapability {
            supported: false,
            clear_thinking_20251015: None,
            clear_tool_uses_20250919: None,
            compact_20260112: None,
            rest: Default::default(),
        },
        effort: claude::EffortCapability {
            supported: false,
            low: None,
            medium: None,
            high: None,
            xhigh: None,
            max: None,
            rest: Default::default(),
        },
        image_input: support(),
        pdf_input: support(),
        structured_outputs: support(),
        thinking: claude::ThinkingCapability {
            supported,
            types: claude::ThinkingTypes {
                adaptive: None,
                enabled: None,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
        rest: Default::default(),
    })
}

pub(crate) fn wire_string<T: serde::Serialize>(value: &T) -> Result<String, TransformError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| TransformError::shape("model id", "expected a string"))
}
