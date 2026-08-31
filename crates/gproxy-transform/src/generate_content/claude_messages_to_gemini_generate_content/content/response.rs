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
            super::text_part(block.text, Default::default())
        }
        claude::ResponseContentBlock::Thinking(block) => thought(block),
        claude::ResponseContentBlock::ToolUse(block) if tools::is_native_name(&block.name) => {
            native::call(block.id, block.input, Default::default())?
        }
        claude::ResponseContentBlock::ToolUse(mut block) => {
            let signature = take_signature(&mut block.caller)?;
            function_call(block, signature)
        }
        claude::ResponseContentBlock::ServerToolUse(block)
            if tools::is_server_native_name(&block.name) =>
        {
            native::call(block.id, block.input, Default::default())?
        }
        claude::ResponseContentBlock::BashCodeExecutionToolResult(block) => {
            native::response_bash_result(block)?
        }
        claude::ResponseContentBlock::Raw(_)
        | claude::ResponseContentBlock::RedactedThinking(_)
        | claude::ResponseContentBlock::ServerToolUse(_)
        | claude::ResponseContentBlock::WebSearchToolResult(_)
        | claude::ResponseContentBlock::WebFetchToolResult(_)
        | claude::ResponseContentBlock::AdvisorToolResult(_)
        | claude::ResponseContentBlock::CodeExecutionToolResult(_)
        | claude::ResponseContentBlock::TextEditorCodeExecutionToolResult(_)
        | claude::ResponseContentBlock::ToolSearchToolResult(_)
        | claude::ResponseContentBlock::McpToolUse(_)
        | claude::ResponseContentBlock::McpToolResult(_)
        | claude::ResponseContentBlock::ContainerUpload(_)
        | claude::ResponseContentBlock::Compaction(_)
        | claude::ResponseContentBlock::Fallback(_) => return Ok(None),
        _future => return Ok(None),
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
        return Ok(None);
    };
    let signature = caller
        .rest
        .remove("thought_signature")
        .or_else(|| caller.rest.remove("thoughtSignature"))
        .and_then(|value| value.as_str().map(str::to_owned));
    let Some(signature) = signature else {
        return Ok(None);
    };
    Ok(Some(signature))
}
