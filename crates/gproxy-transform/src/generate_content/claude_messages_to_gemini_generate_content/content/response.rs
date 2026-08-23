use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::native;
use crate::generate_content::claude_messages_to_gemini_generate_content::tools;

pub(crate) fn response_content(
    blocks: Vec<claude::ContentBlock>,
) -> Result<gemini::Content, TransformError> {
    let mut parts = Vec::new();
    for block in blocks {
        parts.extend(response_block(block)?);
    }
    Ok(super::model_content(parts))
}

pub(crate) fn response_block(
    block: claude::ContentBlock,
) -> Result<Option<gemini::Part>, TransformError> {
    Ok(Some(match block {
        claude::ResponseContentBlock::Text(block) => {
            if block.citations.is_some() || !block.rest.is_empty() {
                return Err(TransformError::unsupported(
                    "Claude response text",
                    "citations or rest",
                ));
            }
            super::text_part(block.text, Default::default())
        }
        claude::ResponseContentBlock::Thinking(block) => {
            if !block.rest.is_empty() {
                return Err(TransformError::unsupported(
                    "Claude response thinking",
                    "rest fields",
                ));
            }
            thought(block)
        }
        claude::ResponseContentBlock::ToolUse(block) if tools::is_native_name(&block.name) => {
            if block.caller.is_some() || !block.rest.is_empty() {
                return Err(TransformError::unsupported(
                    "Claude native tool call",
                    "caller or rest",
                ));
            }
            native::call(block.id, block.input, Default::default())?
        }
        claude::ResponseContentBlock::ToolUse(mut block) => {
            if !block.rest.is_empty() {
                return Err(TransformError::unsupported(
                    "Claude tool call",
                    "rest fields",
                ));
            }
            let signature = take_signature(&mut block.caller)?;
            function_call(block, signature)
        }
        claude::ResponseContentBlock::ServerToolUse(block)
            if tools::is_server_native_name(&block.name) =>
        {
            if block.caller.is_some() || !block.rest.is_empty() {
                return Err(TransformError::unsupported(
                    "Claude server tool call",
                    "caller or rest",
                ));
            }
            native::call(block.id, block.input, Default::default())?
        }
        claude::ResponseContentBlock::BashCodeExecutionToolResult(block) => {
            native::response_bash_result(block)?
        }
        claude::ResponseContentBlock::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude raw response block",
                raw.to_string(),
            ));
        }
        other => {
            return Err(TransformError::unsupported(
                "Claude response block",
                serde_json::to_string(&other)?,
            ));
        }
    }))
}

fn function_call(block: claude::ResponseToolUseBlock, signature: Option<String>) -> gemini::Part {
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

fn thought(block: claude::ThinkingBlock) -> gemini::Part {
    gemini::Part {
        thought: Some(true),
        thought_signature: block.signature,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::Text {
            text: block.thinking,
            rest: Default::default(),
        }),
        metadata: None,
        rest: block.rest,
    }
}

fn take_signature(caller: &mut Option<claude::Caller>) -> Result<Option<String>, TransformError> {
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
        .ok_or_else(|| TransformError::shape("Claude tool caller", "invalid signature"))?;
    if !caller.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Claude tool caller",
            "unmapped caller fields",
        ));
    }
    Ok(Some(signature))
}
