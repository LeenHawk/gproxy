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
) -> Result<(String, openai::Rest), TransformError> {
    let (name, inner_rest) = match kind {
        ToolKind::Function => call
            .function
            .as_ref()
            .map(|function| (function.name.clone(), function.rest.clone())),
        ToolKind::Custom => call
            .custom
            .as_ref()
            .map(|custom| (custom.name.clone(), custom.rest.clone())),
    }
    .ok_or_else(|| TransformError::shape("Chat stream", "tool payload missing"))?;
    let name = name
        .ok_or_else(|| TransformError::shape("Chat stream", "tool name missing on first delta"))?;
    let mut rest = call.rest.clone();
    merge_rest(&mut rest, inner_rest);
    Ok((name, rest))
}

pub(super) fn tool_payload(
    call: openai::ChatToolCallDelta,
    kind: ToolKind,
) -> Result<(String, Option<String>, openai::Rest), TransformError> {
    let mut rest = call.rest;
    let (delta, name, inner_rest) = match kind {
        ToolKind::Function => {
            if call.custom.is_some() {
                return Err(TransformError::shape(
                    "Chat stream",
                    "custom payload on a function tool delta",
                ));
            }
            call.function
                .map(|function| (function.arguments, function.name, function.rest))
        }
        ToolKind::Custom => {
            if call.function.is_some() {
                return Err(TransformError::shape(
                    "Chat stream",
                    "function payload on a custom tool delta",
                ));
            }
            call.custom
                .map(|custom| (custom.input, custom.name, custom.rest))
        }
    }
    .unwrap_or((None, None, Default::default()));
    merge_rest(&mut rest, inner_rest);
    Ok((delta.unwrap_or_default(), name, rest))
}

pub(super) fn merge_rest(target: &mut openai::Rest, source: openai::Rest) {
    target.extend(source);
}
