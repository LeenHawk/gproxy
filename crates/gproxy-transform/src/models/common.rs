use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(super) fn claude_to_openai(model: claude::ModelInfo) -> Result<openai::Model, TransformError> {
    let mut rest = model.rest;
    preserve(&mut rest, "created_at", &model.created_at)?;
    preserve(
        &mut rest,
        "allowed_fallback_models",
        &model.allowed_fallback_models,
    )?;
    preserve(&mut rest, "capabilities", &model.capabilities)?;
    let carried_thinking = take(&mut rest, "thinking_supported")?;
    let thinking_supported = carried_thinking.or_else(|| {
        model
            .capabilities
            .as_ref()
            .map(|capabilities| capabilities.thinking.supported)
    });
    Ok(openai::Model {
        id: wire_string(&model.id)?.into(),
        created: take(&mut rest, "created")?,
        display_name: model.display_name,
        context_window: model.max_input_tokens,
        max_context_window: take(&mut rest, "max_context_window")?,
        max_output_tokens: model.max_tokens,
        thinking_supported,
        object: openai::ModelObjectType::Model,
        owned_by: take(&mut rest, "owned_by")?.or_else(|| Some("unknown".into())),
        rest,
    })
}

pub(super) fn openai_to_claude(model: openai::Model) -> Result<claude::ModelInfo, TransformError> {
    let mut rest = model.rest;
    let id = wire_string(&model.id)?;
    preserve(&mut rest, "created", &model.created)?;
    preserve(&mut rest, "owned_by", &model.owned_by)?;
    preserve(&mut rest, "max_context_window", &model.max_context_window)?;
    preserve(&mut rest, "thinking_supported", &model.thinking_supported)?;
    Ok(claude::ModelInfo {
        id: id.clone().into(),
        allowed_fallback_models: take(&mut rest, "allowed_fallback_models")?,
        type_: claude::ModelObjectType::Known(claude::ModelObjectTypeKnown::Model),
        created_at: take(&mut rest, "created_at")?.or_else(|| Some("1970-01-01T00:00:00Z".into())),
        display_name: model.display_name.or(Some(id)),
        max_input_tokens: model.context_window.or(model.max_context_window),
        max_tokens: model.max_output_tokens,
        capabilities: take(&mut rest, "capabilities")?,
        rest,
    })
}

pub(super) fn take<T: serde::de::DeserializeOwned>(
    rest: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<Option<T>, TransformError> {
    rest.remove(name)
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}

pub(super) fn preserve<T: serde::Serialize>(
    rest: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: &T,
) -> Result<(), TransformError> {
    let value = serde_json::to_value(value)?;
    if !value.is_null() {
        rest.insert(name.into(), value);
    }
    Ok(())
}

pub(crate) fn wire_string<T: serde::Serialize>(value: &T) -> Result<String, TransformError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| TransformError::shape("model id", "expected a string"))
}
