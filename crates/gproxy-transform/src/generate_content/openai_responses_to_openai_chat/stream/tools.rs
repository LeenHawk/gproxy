use gproxy_protocol::openai;

use crate::TransformError;

use super::ToolKind;

pub(super) fn tool_kind(call: &openai::ChatToolCallDelta) -> Result<ToolKind, TransformError> {
    let from_type = match call.type_.as_ref() {
        Some(openai::ChatToolCallType::Function) => Some(ToolKind::Function),
        Some(openai::ChatToolCallType::Custom) => Some(ToolKind::Custom),
        Some(openai::ChatToolCallType::Unknown(value)) => {
            return Err(TransformError::unsupported("Chat tool call type", value));
        }
        None => None,
    };
    let from_payload = match (call.function.is_some(), call.custom.is_some()) {
        (true, false) => Some(ToolKind::Function),
        (false, true) => Some(ToolKind::Custom),
        (false, false) => None,
        (true, true) => {
            return Err(TransformError::shape(
                "Chat stream",
                "tool delta contains function and custom payloads",
            ));
        }
    };
    match (from_type, from_payload) {
        (Some(left), Some(right)) if left != right => Err(TransformError::shape(
            "Chat stream",
            "tool delta type does not match its payload",
        )),
        (Some(kind), _) | (_, Some(kind)) => Ok(kind),
        (None, None) => Err(TransformError::shape(
            "Chat stream",
            "tool kind missing on first delta",
        )),
    }
}

pub(super) fn tool_kind_or(
    call: &openai::ChatToolCallDelta,
    fallback: ToolKind,
) -> Result<ToolKind, TransformError> {
    if call.type_.is_none() && call.function.is_none() && call.custom.is_none() {
        Ok(fallback)
    } else {
        tool_kind(call)
    }
}

pub(super) fn tool_metadata(
    call: &openai::ChatToolCallDelta,
    kind: ToolKind,
) -> Result<String, TransformError> {
    let name = match kind {
        ToolKind::Function => call
            .function
            .as_ref()
            .and_then(|function| function.name.clone()),
        ToolKind::Custom => call.custom.as_ref().and_then(|custom| custom.name.clone()),
    }
    .ok_or_else(|| TransformError::shape("Chat stream", "tool payload missing"))?;
    Ok(name)
}

pub(super) fn tool_payload(
    call: openai::ChatToolCallDelta,
    kind: ToolKind,
) -> Result<(String, Option<String>), TransformError> {
    let (delta, name) = match kind {
        ToolKind::Function => {
            if call.custom.is_some() {
                return Err(TransformError::shape(
                    "Chat stream",
                    "custom payload on a function tool delta",
                ));
            }
            call.function
                .map(|function| (function.arguments, function.name))
        }
        ToolKind::Custom => {
            if call.function.is_some() {
                return Err(TransformError::shape(
                    "Chat stream",
                    "function payload on a custom tool delta",
                ));
            }
            call.custom.map(|custom| (custom.input, custom.name))
        }
    }
    .unwrap_or((None, None));
    Ok((delta.unwrap_or_default(), name))
}
