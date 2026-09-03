use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::native;
use crate::generate_content::claude_messages_to_gemini_generate_content::tools;

pub(crate) fn response_content(
    blocks: Vec<claude::ContentBlock>,
) -> Result<gemini::Content, TransformError> {
    let mut parts = Vec::new();
    let mut pending_signature = None;
    for block in blocks {
        match &block {
            claude::ResponseContentBlock::Thinking(block) => {
                pending_signature = block.signature.clone();
            }
            claude::ResponseContentBlock::RedactedThinking(block) => {
                pending_signature = Some(block.data.clone());
                parts.push(signature_part(block.data.clone()));
                continue;
            }
            _ => {}
        }
        if let Some(mut part) = response_block(block)? {
            attach_signature(&mut part, &mut pending_signature);
            parts.push(part);
        }
    }
    Ok(super::model_content(parts))
}

pub(crate) fn attach_signature(part: &mut gemini::Part, pending: &mut Option<String>) {
    if matches!(part.data, Some(gemini::PartData::FunctionCall { .. })) {
        let inherited = pending.take();
        if part.thought_signature.is_none() {
            part.thought_signature = inherited;
        }
    }
}

pub(crate) fn signature_part(signature: String) -> gemini::Part {
    crate::wire!(gemini::Part {
        thought: Some(true),
        thought_signature: Some(signature),
        part_metadata: None,
        media_resolution: None,
        data: None,
        metadata: None,
        rest: Default::default(),
    })
}

pub(crate) fn response_block(
    block: claude::ContentBlock,
) -> Result<Option<gemini::Part>, TransformError> {
    Ok(Some(match block {
        claude::ResponseContentBlock::Text(block) => super::text_part(block.text),
        claude::ResponseContentBlock::Thinking(block) => thought(block),
        claude::ResponseContentBlock::ToolUse(block) if tools::is_native_name(&block.name) => {
            native::call(block.id, block.input)?
        }
        claude::ResponseContentBlock::ToolUse(block) => function_call(block, None),
        claude::ResponseContentBlock::ServerToolUse(block)
            if tools::is_server_native_name(&block.name) =>
        {
            native::call(block.id, block.input)?
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
    crate::wire!(gemini::Part {
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
        rest: Default::default(),
    })
}

fn thought(block: claude::ThinkingBlock) -> gemini::Part {
    crate::wire!(gemini::Part {
        thought: Some(true),
        thought_signature: block.signature,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::Text {
            text: block.thinking,
            rest: Default::default(),
        }),
        metadata: None,
        rest: Default::default(),
    })
}
