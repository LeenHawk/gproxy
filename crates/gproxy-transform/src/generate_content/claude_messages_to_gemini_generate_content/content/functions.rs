use std::collections::BTreeMap;

use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::native;

pub(super) fn function_call(
    block: claude::ToolUseBlock,
    signature: Option<String>,
) -> gemini::Part {
    gemini::Part {
        thought: None,
        thought_signature: signature,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::FunctionCall {
            function_call: gemini::FunctionCall {
                id: Some(block.id),
                name: block.name,
                args: Some(block.input),
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        metadata: None,
        rest: block.rest,
    }
}

pub(super) fn function_result(
    block: claude::ToolResultBlock,
    names: &BTreeMap<String, String>,
) -> Result<gemini::Part, TransformError> {
    let name = names.get(&block.tool_use_id).cloned().ok_or_else(|| {
        TransformError::shape("Claude tool result", "matching tool name is missing")
    })?;
    let mut response = gemini::JsonMap::new();
    if let Some(content) = block.content {
        let key = if block.is_error == Some(true) {
            "error"
        } else {
            "output"
        };
        response.insert(key.into(), native::result_text(content)?.into());
    } else if block.is_error == Some(true) {
        return Err(TransformError::shape(
            "Claude tool result",
            "error result content is missing",
        ));
    }
    Ok(part(gemini::PartData::FunctionResponse {
        function_response: gemini::FunctionResponse {
            id: Some(block.tool_use_id),
            name,
            response,
            parts: None,
            will_continue: None,
            scheduling: None,
            rest: Default::default(),
        },
        rest: Default::default(),
    }))
}

pub(super) fn thought(block: claude::ThinkingBlock) -> gemini::Part {
    let mut part = part(gemini::PartData::Text {
        text: block.thinking,
        rest: Default::default(),
    });
    part.thought = Some(true);
    part.thought_signature = block.signature;
    part.rest = block.rest;
    part
}

pub(super) fn take_signature(
    caller: &mut Option<claude::Caller>,
) -> Result<Option<String>, TransformError> {
    let Some(caller) = caller.as_mut() else {
        return Ok(None);
    };
    let claude::Caller::Direct(caller) = caller else {
        return Err(TransformError::unsupported(
            "Claude tool caller",
            "non-direct caller",
        ));
    };
    let signature = caller
        .rest
        .remove("thought_signature")
        .or_else(|| caller.rest.remove("thoughtSignature"))
        .ok_or_else(|| TransformError::unsupported("Claude tool caller", "missing signature"))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| TransformError::shape("Claude tool caller", "signature is not a string"))?;
    if !caller.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Claude tool caller",
            "unmapped caller fields",
        ));
    }
    Ok(Some(signature))
}

fn part(data: gemini::PartData) -> gemini::Part {
    gemini::Part {
        thought: None,
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(data),
        metadata: None,
        rest: Default::default(),
    }
}
