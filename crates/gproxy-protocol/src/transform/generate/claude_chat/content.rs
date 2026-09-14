use super::media::{document_file, image_url, openai_user_blocks};
use super::util::parse_tool_arguments;
use std::borrow::Cow;

use crate::{
    Rest,
    transform::{Report, TransformError},
    wire::{claude::content as c, openai::chat},
};

pub(super) fn text_block(text: String) -> c::ContentBlock {
    c::ContentBlock::Text(c::TextBlock {
        type_: c::TextBlockType::Tag,
        text,
        cache_control: None,
        citations: None,
        rest: Rest::new(),
    })
}

pub(crate) fn claude_message_to_openai(
    message: &c::Message,
    report: &mut Report,
) -> Result<Vec<chat::ChatMessage>, TransformError> {
    match message.role {
        c::Role::System => Ok(vec![chat::ChatMessage::System(chat::SystemMessage {
            role: chat::SystemRole::System,
            content: claude_text(&message.content)?,
            name: None,
            rest: Rest::new(),
        })]),
        c::Role::User => {
            let mut parts = Vec::new();
            let mut output = Vec::new();
            for block in claude_blocks(&message.content)?.iter() {
                match block {
                    c::ContentBlock::Text(block) => {
                        parts.push(chat::UserContentPart::Text(chat::TextPart {
                            type_: chat::TextPartType::Text,
                            text: block.text.clone(),
                            prompt_cache_breakpoint: None,
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::Image(block) => {
                        parts.push(chat::UserContentPart::Image(chat::ImagePart {
                            type_: chat::ImagePartType::ImageUrl,
                            image_url: image_url(&block.source)?,
                            prompt_cache_breakpoint: None,
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::Document(block) => {
                        parts.push(chat::UserContentPart::File(chat::FilePart {
                            type_: chat::FilePartType::File,
                            file: document_file(block)?,
                            prompt_cache_breakpoint: None,
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::ToolResult(block) => {
                        let text = match &block.content {
                            Some(c::ToolResultContent::Text(text)) => text.clone(),
                            Some(c::ToolResultContent::Blocks(blocks)) => blocks
                                .iter()
                                .map(|block| match block {
                                    c::ToolResultContentBlock::Text(text) => Ok(text.text.clone()),
                                    c::ToolResultContentBlock::ToolReference(reference) => {
                                        if reference.tool_name.trim().is_empty() {
                                            return Err(TransformError::shape("tool_result.tool_reference.tool_name", "non-empty tool name required"));
                                        }
                                        report.changed("tool_result.tool_reference", "tool reference represented as text; callable schema remains in tools");
                                        if reference.cache_control.is_some() {
                                            report.omitted("tool_result.tool_reference.cache_control", "Chat tool results have no Claude cache field");
                                        }
                                        Ok(serde_json::json!({"type":"tool_reference","tool_name":reference.tool_name}).to_string())
                                    }
                                    c::ToolResultContentBlock::Image(_) | c::ToolResultContentBlock::SearchResult(_) | c::ToolResultContentBlock::Document(_) => Err(TransformError::unsupported("tool_result.content", "Chat tool messages cannot represent non-text result blocks")),
                                })
                                .collect::<Result<Vec<_>, _>>()?
                                .join(""),
                            None => String::new(),
                        };
                        if !parts.is_empty() {
                            output.push(chat::ChatMessage::User(chat::UserMessage {
                                role: chat::UserRole::User,
                                content: chat::UserContent::Parts(std::mem::take(&mut parts)),
                                name: None,
                                rest: Rest::new(),
                            }));
                        }
                        let text = if block.is_error == Some(true) {
                            report.changed(
                                "tool_result.is_error",
                                "error status is represented in the Chat tool result text",
                            );
                            format!("Tool error:\n{text}")
                        } else {
                            text
                        };
                        output.push(chat::ChatMessage::Tool(chat::ToolMessage {
                            role: chat::ToolRole::Tool,
                            content: chat::TextContent::Text(text),
                            tool_call_id: super::util::request_id(&block.tool_use_id)?,
                            rest: Rest::new(),
                        }));
                    }
                    c::ContentBlock::SearchResult(block) => {
                        if block
                            .citations
                            .as_ref()
                            .is_some_and(|config| config.enabled == Some(true))
                        {
                            return Err(TransformError::unsupported(
                                "search_result.citations",
                                "Chat has no Claude search-result citation control",
                            ));
                        }
                        report.changed(
                            "search_result",
                            "source/title/content are represented as labeled text",
                        );
                        let text = format!(
                            "{}\n{}\n{}",
                            block.title,
                            block.source,
                            block
                                .content
                                .iter()
                                .map(|text| text.text.as_str())
                                .collect::<Vec<_>>()
                                .join("")
                        );
                        parts.push(chat::UserContentPart::Text(
                            chat::TextPart::builder(chat::TextPartType::Text, text).build(),
                        ));
                    }
                    c::ContentBlock::Thinking(_) | c::ContentBlock::RedactedThinking(_) => report
                        .changed(
                            "messages.user.content",
                            "Claude reasoning/search block has no Chat user-part equivalent",
                        ),
                    c::ContentBlock::ToolUse(_)
                    | c::ContentBlock::ServerToolUse(_)
                    | c::ContentBlock::McpToolUse(_)
                    | c::ContentBlock::McpToolResult(_)
                    | c::ContentBlock::WebSearchToolResult(_)
                    | c::ContentBlock::WebFetchToolResult(_)
                    | c::ContentBlock::AdvisorToolResult(_)
                    | c::ContentBlock::CodeExecutionToolResult(_)
                    | c::ContentBlock::BashCodeExecutionToolResult(_)
                    | c::ContentBlock::TextEditorCodeExecutionToolResult(_)
                    | c::ContentBlock::ToolSearchToolResult(_) => {
                        return Err(TransformError::unsupported(
                            "messages.user.content",
                            "Claude tool block requires an explicit Chat binding",
                        ));
                    }
                    c::ContentBlock::ContainerUpload(_)
                    | c::ContentBlock::Compaction(_)
                    | c::ContentBlock::MidConversationSystem(_)
                    | c::ContentBlock::ToolAddition(_)
                    | c::ContentBlock::ToolRemoval(_)
                    | c::ContentBlock::Fallback(_) => {
                        return Err(TransformError::unsupported(
                            "messages.user.content",
                            "Claude state-changing or opaque block requires a host adapter",
                        ));
                    }
                }
            }
            if !parts.is_empty() {
                output.push(chat::ChatMessage::User(chat::UserMessage {
                    role: chat::UserRole::User,
                    content: chat::UserContent::Parts(parts),
                    name: None,
                    rest: Rest::new(),
                }));
            }
            Ok(output)
        }
        c::Role::Assistant => {
            let mut parts = Vec::new();
            let mut calls = Vec::new();
            for block in claude_blocks(&message.content)?.iter() {
                match block {
                    c::ContentBlock::Text(block) => {
                        parts.push(chat::AssistantContentPart::Text(chat::TextPart {
                            type_: chat::TextPartType::Text,
                            text: block.text.clone(),
                            prompt_cache_breakpoint: None,
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::ToolUse(block) => {
                        calls.push(chat::MessageToolCall::Function(chat::ChatToolCall {
                            id: super::util::request_id(&block.id)?,
                            function: chat::FunctionCall {
                                arguments: serde_json::to_string(&block.input)?,
                                name: block.name.clone(),
                                rest: Rest::new(),
                            },
                            type_: chat::ChatToolCallType::Function,
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::Thinking(_) | c::ContentBlock::RedactedThinking(_) => report
                        .changed(
                            "messages.assistant.content",
                            "Claude reasoning block has no Chat assistant equivalent",
                        ),
                    c::ContentBlock::ToolResult(_)
                    | c::ContentBlock::ServerToolUse(_)
                    | c::ContentBlock::McpToolUse(_)
                    | c::ContentBlock::McpToolResult(_)
                    | c::ContentBlock::WebSearchToolResult(_)
                    | c::ContentBlock::WebFetchToolResult(_)
                    | c::ContentBlock::AdvisorToolResult(_)
                    | c::ContentBlock::CodeExecutionToolResult(_)
                    | c::ContentBlock::BashCodeExecutionToolResult(_)
                    | c::ContentBlock::TextEditorCodeExecutionToolResult(_)
                    | c::ContentBlock::ToolSearchToolResult(_) => {
                        return Err(TransformError::unsupported(
                            "messages.assistant.content",
                            "Claude tool result block has no safe Chat assistant equivalent",
                        ));
                    }
                    c::ContentBlock::Image(_)
                    | c::ContentBlock::Document(_)
                    | c::ContentBlock::SearchResult(_)
                    | c::ContentBlock::ContainerUpload(_)
                    | c::ContentBlock::Compaction(_)
                    | c::ContentBlock::MidConversationSystem(_)
                    | c::ContentBlock::ToolAddition(_)
                    | c::ContentBlock::ToolRemoval(_)
                    | c::ContentBlock::Fallback(_) => {
                        return Err(TransformError::unsupported(
                            "messages.assistant.content",
                            "Claude non-text or state-changing block has no Chat assistant field",
                        ));
                    }
                }
            }
            Ok(vec![chat::ChatMessage::Assistant(chat::AssistantMessage {
                role: chat::AssistantRole::Assistant,
                content: (!parts.is_empty()).then_some(Some(chat::AssistantContent::Parts(parts))),
                audio: None,
                refusal: None,
                name: None,
                tool_calls: (!calls.is_empty()).then_some(calls),
                function_call: None,
                rest: Rest::new(),
            })])
        }
    }
}

pub(crate) fn openai_message_to_claude(
    message: &chat::ChatMessage,
    report: &mut Report,
) -> Result<Vec<c::Message>, TransformError> {
    match message {
        chat::ChatMessage::System(message) => Ok(vec![c::Message {
            role: c::Role::System,
            content: c::MessageContent::Text(chat_text(&message.content)),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::Developer(message) => Ok(vec![c::Message {
            role: c::Role::System,
            content: c::MessageContent::Text(chat_text(&message.content)),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::User(message) => Ok(vec![c::Message {
            role: c::Role::User,
            content: c::MessageContent::Blocks(openai_user_blocks(&message.content, report)?),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::Tool(message) => Ok(vec![c::Message {
            role: c::Role::User,
            content: c::MessageContent::Blocks(vec![c::ContentBlock::ToolResult(
                c::ToolResultBlock {
                    type_: c::ToolResultBlockType::Tag,
                    tool_use_id: super::util::request_id(&message.tool_call_id)?,
                    content: Some(c::ToolResultContent::Text(chat_text(&message.content))),
                    is_error: Some(false),
                    cache_control: None,
                    rest: Rest::new(),
                },
            )]),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::Function(_message) => Err(TransformError::unsupported(
            "messages.function",
            "legacy function messages have no call id",
        )),
        chat::ChatMessage::Assistant(message) => {
            if message.audio.as_ref().and_then(Option::as_ref).is_some() {
                return Err(TransformError::unsupported(
                    "messages.assistant.audio",
                    "Claude has no audio-history reference",
                ));
            }
            if message
                .function_call
                .as_ref()
                .and_then(Option::as_ref)
                .is_some()
            {
                return Err(TransformError::missing_metadata(
                    "messages.assistant.function_call requires stable tool-call identity",
                ));
            }
            let mut blocks = Vec::new();
            if let Some(Some(content)) = &message.content {
                match content {
                    chat::AssistantContent::Text(text) => blocks.push(text_block(text.clone())),
                    chat::AssistantContent::Parts(parts) => {
                        for part in parts {
                            match part {
                                chat::AssistantContentPart::Text(text) => {
                                    blocks.push(text_block(text.text.clone()))
                                }
                                chat::AssistantContentPart::Refusal(refusal) => {
                                    blocks.push(text_block(refusal.refusal.clone()))
                                }
                            }
                        }
                    }
                }
            }
            if let Some(calls) = &message.tool_calls {
                for call in calls {
                    if let chat::MessageToolCall::Function(call) = call {
                        blocks.push(c::ContentBlock::ToolUse(c::ToolUseBlock {
                            type_: c::ToolUseBlockType::Tag,
                            id: super::util::request_id(&call.id)?,
                            input: parse_tool_arguments(
                                &call.function.arguments,
                                "tool_calls.arguments",
                            )?,
                            name: call.function.name.clone(),
                            caller: None,
                            cache_control: None,
                            rest: Rest::new(),
                        }));
                    } else {
                        return Err(TransformError::unsupported(
                            "tool_calls.custom",
                            "custom Chat tool calls require an explicit Claude custom-tool binding",
                        ));
                    }
                }
            }
            if let Some(Some(refusal)) = &message.refusal {
                blocks.push(text_block(refusal.clone()));
            }
            Ok(vec![c::Message {
                role: c::Role::Assistant,
                content: c::MessageContent::Blocks(blocks),
                rest: Rest::new(),
            }])
        }
    }
}

fn claude_text(content: &c::MessageContent) -> Result<chat::TextContent, TransformError> {
    Ok(chat::TextContent::Text(match content {
        c::MessageContent::Text(text) => text.clone(),
        c::MessageContent::Blocks(blocks) => blocks
            .iter()
            .map(|block| match block {
                c::ContentBlock::Text(text) => Ok(text.text.clone()),
                c::ContentBlock::Image(_)
                | c::ContentBlock::Document(_)
                | c::ContentBlock::Thinking(_)
                | c::ContentBlock::RedactedThinking(_)
                | c::ContentBlock::ToolUse(_)
                | c::ContentBlock::ToolResult(_)
                | c::ContentBlock::ServerToolUse(_)
                | c::ContentBlock::SearchResult(_)
                | c::ContentBlock::WebSearchToolResult(_)
                | c::ContentBlock::WebFetchToolResult(_)
                | c::ContentBlock::AdvisorToolResult(_)
                | c::ContentBlock::CodeExecutionToolResult(_)
                | c::ContentBlock::BashCodeExecutionToolResult(_)
                | c::ContentBlock::TextEditorCodeExecutionToolResult(_)
                | c::ContentBlock::ToolSearchToolResult(_)
                | c::ContentBlock::McpToolUse(_)
                | c::ContentBlock::McpToolResult(_)
                | c::ContentBlock::ContainerUpload(_)
                | c::ContentBlock::Compaction(_)
                | c::ContentBlock::MidConversationSystem(_)
                | c::ContentBlock::ToolAddition(_)
                | c::ContentBlock::ToolRemoval(_)
                | c::ContentBlock::Fallback(_) => Err(TransformError::unsupported(
                    "messages.system",
                    "Chat system messages cannot represent non-text blocks",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(""),
    }))
}

fn claude_blocks(
    content: &c::MessageContent,
) -> Result<Cow<'_, [c::ContentBlock]>, TransformError> {
    Ok(match content {
        c::MessageContent::Text(text) => Cow::Owned(vec![text_block(text.clone())]),
        c::MessageContent::Blocks(blocks) => Cow::Borrowed(blocks),
    })
}

pub(super) fn chat_text(content: &chat::TextContent) -> String {
    match content {
        chat::TextContent::Text(text) => text.clone(),
        chat::TextContent::Parts(parts) => parts
            .iter()
            .map(|part| part.text.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
