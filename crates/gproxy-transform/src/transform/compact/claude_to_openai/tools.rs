use crate::protocol::{claude, openai};

use super::input::ClaudeRequestBlockItem;
use super::util::{
    document_source_to_input_part, image_source_to_input_part, join_text, json_object_to_string,
    server_tool_name_to_string,
};

pub(super) fn server_tool_use_item(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseName,
) -> ClaudeRequestBlockItem {
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(
        openai::TypedResponseItem::FunctionCall {
            arguments: json_object_to_string(&input),
            call_id: id.clone(),
            name: server_tool_name_to_string(&name),
            id: Some(id),
            namespace: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            extra: Default::default(),
        },
    ))
}

pub(super) fn function_call_output_item(
    call_id: String,
    output: openai::ResponseOutput,
) -> ClaudeRequestBlockItem {
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(
        openai::TypedResponseItem::FunctionCallOutput {
            call_id,
            output,
            id: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            extra: Default::default(),
        },
    ))
}

pub(super) fn compact_server_tool_use_item(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseName,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
        arguments: json_object_to_string(&input),
        call_id: id.clone(),
        name: server_tool_name_to_string(&name),
        id: Some(id),
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        extra: Default::default(),
    })
}

pub(super) fn compact_function_call_output_item(
    call_id: String,
    output: openai::ResponseOutput,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(openai::TypedResponseItem::FunctionCallOutput {
        call_id,
        output,
        id: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        created_by: None,
        extra: Default::default(),
    })
}

pub(super) fn tool_result_content_to_openai(
    content: Option<claude::ToolResultContent>,
) -> openai::ResponseOutput {
    match content {
        Some(claude::ToolResultContent::Text(text)) => openai::ResponseOutput::Text(text),
        Some(claude::ToolResultContent::Blocks(blocks)) => {
            let parts = blocks
                .into_iter()
                .filter_map(tool_result_block_to_openai)
                .collect::<Vec<_>>();
            openai::ResponseOutput::Parts(parts)
        }
        Some(claude::ToolResultContent::Raw(value)) => {
            openai::ResponseOutput::Text(value.to_string())
        }
        None => openai::ResponseOutput::Text(String::new()),
    }
}

pub(super) fn server_tool_result_output<T: serde::Serialize>(
    content: &T,
) -> openai::ResponseOutput {
    openai::ResponseOutput::Text(serde_json::to_string(content).unwrap_or_else(|_| String::new()))
}

pub(super) fn mcp_tool_result_content_to_text(
    content: Option<claude::McpToolResultContent>,
) -> String {
    match content {
        Some(claude::McpToolResultContent::String(text)) => text,
        Some(claude::McpToolResultContent::Array(blocks)) => {
            join_text(blocks.into_iter().map(|block| block.text))
        }
        None => String::new(),
    }
}

pub(super) fn response_mcp_tool_result_content_to_text(
    content: claude::ResponseMcpToolResultContent,
) -> String {
    match content {
        claude::ResponseMcpToolResultContent::String(text) => text,
        claude::ResponseMcpToolResultContent::Array(blocks) => {
            join_text(blocks.into_iter().map(|block| block.text))
        }
    }
}

fn tool_result_block_to_openai(
    block: claude::ToolResultContentBlock,
) -> Option<openai::ResponseToolOutputContentPart> {
    match block {
        claude::ToolResultContentBlock::Text(block) => {
            Some(openai::ResponseToolOutputContentPart::InputText {
                text: block.text,
                extra: Default::default(),
            })
        }
        claude::ToolResultContentBlock::Image(block) => {
            input_part_to_tool_output_part(image_source_to_input_part(block.source)?)
        }
        claude::ToolResultContentBlock::Document(block) => input_part_to_tool_output_part(
            document_source_to_input_part(block.source, block.title)?,
        ),
        claude::ToolResultContentBlock::SearchResult(block) => {
            let text = join_text(
                block
                    .content
                    .into_iter()
                    .map(|content_block| content_block.text)
                    .chain([block.source, block.title]),
            );
            (!text.is_empty()).then_some(openai::ResponseToolOutputContentPart::InputText {
                text,
                extra: Default::default(),
            })
        }
        claude::ToolResultContentBlock::ToolReference(_)
        | claude::ToolResultContentBlock::Raw(_) => None,
    }
}

fn input_part_to_tool_output_part(
    part: openai::ResponseInputContentPart,
) -> Option<openai::ResponseToolOutputContentPart> {
    match part {
        openai::ResponseInputContentPart::InputText { text, .. } => {
            Some(openai::ResponseToolOutputContentPart::InputText {
                text,
                extra: Default::default(),
            })
        }
        openai::ResponseInputContentPart::InputImage {
            detail,
            file_id,
            image_url,
            ..
        } => Some(openai::ResponseToolOutputContentPart::InputImage {
            detail,
            file_id,
            image_url,
            extra: Default::default(),
        }),
        openai::ResponseInputContentPart::InputFile {
            detail,
            file_data,
            file_id,
            file_url,
            filename,
            ..
        } => Some(openai::ResponseToolOutputContentPart::InputFile {
            detail,
            file_data,
            file_id,
            file_url,
            filename,
            extra: Default::default(),
        }),
        openai::ResponseInputContentPart::InputAudio { .. } => None,
    }
}
