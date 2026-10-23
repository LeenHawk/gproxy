use super::media::{document_file, image_url, openai_user_blocks};
use super::util::parse_tool_arguments;
use crate::transform::generate::{prompt_cache as cache, reasoning_details as rd};
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
                            prompt_cache_breakpoint: block
                                .cache_control
                                .as_ref()
                                .map(cache::claude_to_chat),
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::Image(block) => {
                        let Some(image_url) = crate::transform::optional(image_url(&block.source))?
                        else {
                            continue;
                        };
                        parts.push(chat::UserContentPart::Image(chat::ImagePart {
                            type_: chat::ImagePartType::ImageUrl,
                            image_url,
                            prompt_cache_breakpoint: block
                                .cache_control
                                .as_ref()
                                .map(cache::claude_to_chat),
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::Document(block) => {
                        let Some(file) = crate::transform::optional(document_file(block))? else {
                            continue;
                        };
                        parts.push(chat::UserContentPart::File(chat::FilePart {
                            type_: chat::FilePartType::File,
                            file,
                            prompt_cache_breakpoint: block
                                .cache_control
                                .as_ref()
                                .map(cache::claude_to_chat),
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::ToolResult(v) if v.toolset_name.is_some() => {
                        report.omitted(
                            "toolset_name",
                            "native toolset member has no target definition",
                        );
                        continue;
                    }
                    c::ContentBlock::ToolResult(block) => {
                        let content = tool_result_text(block, report)?;
                        if !parts.is_empty() {
                            output.push(chat::ChatMessage::User(chat::UserMessage {
                                role: chat::UserRole::User,
                                content: chat::UserContent::Parts(std::mem::take(&mut parts)),
                                name: None,
                                rest: Rest::new(),
                            }));
                        }
                        output.push(chat::ChatMessage::Tool(chat::ToolMessage {
                            role: chat::ToolRole::Tool,
                            content,
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
                            report
                                .omitted("search_result.citations", "Chat has no citation control");
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
                    c::ContentBlock::Thinking(_) => report.omitted(
                        "messages.user.thinking",
                        "reasoning requires assistant role",
                    ),
                    c::ContentBlock::RedactedThinking(_) => report.changed(
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
                        report.omitted(
                            "messages.user.content",
                            "block has no target representation",
                        );
                    }
                    c::ContentBlock::ContainerUpload(_)
                    | c::ContentBlock::McpToolListing(_)
                    | c::ContentBlock::Compaction(_)
                    | c::ContentBlock::MidConversationSystem(_)
                    | c::ContentBlock::ToolAddition(_)
                    | c::ContentBlock::ToolRemoval(_)
                    | c::ContentBlock::Fallback(_) => {
                        report.omitted(
                            "messages.user.content",
                            "block has no target representation",
                        );
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
            let mut reasoning = Vec::new();
            let mut details = Vec::new();
            let mut parts = Vec::new();
            let mut calls = Vec::new();
            for (index, block) in claude_blocks(&message.content)?.iter().enumerate() {
                match block {
                    c::ContentBlock::Text(block) => {
                        parts.push(chat::AssistantContentPart::Text(chat::TextPart {
                            type_: chat::TextPartType::Text,
                            text: block.text.clone(),
                            prompt_cache_breakpoint: block
                                .cache_control
                                .as_ref()
                                .map(cache::claude_to_chat),
                            rest: Rest::new(),
                        }))
                    }
                    c::ContentBlock::ToolUse(v) if v.toolset_name.is_some() => {
                        report.omitted(
                            "toolset_name",
                            "native toolset member has no target definition",
                        );
                        continue;
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
                    // A signature carried from another upstream (a Gemini thought)
                    // only goes back to that upstream; this one would refuse it.
                    c::ContentBlock::Thinking(block)
                        if crate::transform::generate::signature::is_carried(&block.signature) =>
                    {
                        report.omitted(
                            "messages.assistant.thinking",
                            "a signature carried from another upstream only goes back to that upstream",
                        );
                    }
                    c::ContentBlock::Thinking(block) => {
                        reasoning.push(block.thinking.clone());
                        details.push(rd::from_thinking(block, index as i64));
                    }
                    c::ContentBlock::RedactedThinking(block) => {
                        details.push(rd::from_redacted(block, index as i64))
                    }
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
                        report.omitted(
                            "messages.assistant.content",
                            "block has no target representation",
                        );
                    }
                    c::ContentBlock::Image(_)
                    | c::ContentBlock::Document(_)
                    | c::ContentBlock::SearchResult(_)
                    | c::ContentBlock::ContainerUpload(_)
                    | c::ContentBlock::McpToolListing(_)
                    | c::ContentBlock::Compaction(_)
                    | c::ContentBlock::MidConversationSystem(_)
                    | c::ContentBlock::ToolAddition(_)
                    | c::ContentBlock::ToolRemoval(_)
                    | c::ContentBlock::Fallback(_) => {
                        report.omitted(
                            "messages.assistant.content",
                            "block has no target representation",
                        );
                    }
                }
            }
            Ok(vec![chat::ChatMessage::Assistant(chat::AssistantMessage {
                reasoning_details: (!details.is_empty()).then_some(Some(details)),
                reasoning_content: (!reasoning.is_empty()).then(|| Some(reasoning.join(""))),
                reasoning: None,
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
            clear_at: None,
            output_config: None,
            role: c::Role::System,
            content: chat_content(&message.content),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::Developer(message) => Ok(vec![c::Message {
            clear_at: None,
            output_config: None,
            role: c::Role::System,
            content: chat_content(&message.content),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::User(message) => Ok(vec![c::Message {
            clear_at: None,
            output_config: None,
            role: c::Role::User,
            content: c::MessageContent::Blocks(openai_user_blocks(&message.content, report)?),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::Tool(message) => Ok(vec![c::Message {
            clear_at: None,
            output_config: None,
            role: c::Role::User,
            content: c::MessageContent::Blocks(vec![c::ContentBlock::ToolResult(
                c::ToolResultBlock {
                    toolset_name: None,
                    type_: c::ToolResultBlockType::Tag,
                    tool_use_id: super::util::request_id(&message.tool_call_id)?,
                    content: Some(chat_result(&message.content)),
                    is_error: Some(false),
                    cache_control: None,
                    rest: Rest::new(),
                },
            )]),
            rest: Rest::new(),
        }]),
        chat::ChatMessage::Function(_) => {
            report.omitted(
                "messages.function",
                "legacy message has no recoverable call identity",
            );
            Ok(Vec::new())
        }
        chat::ChatMessage::Assistant(message) => {
            if message.audio.as_ref().and_then(Option::as_ref).is_some() {
                report.omitted(
                    "messages.assistant.audio",
                    "field has no target representation",
                );
            }
            if message
                .function_call
                .as_ref()
                .and_then(Option::as_ref)
                .is_some()
            {
                report.omitted(
                    "messages.assistant.function_call",
                    "no recoverable call identity",
                );
            }
            let mut blocks = rd::to_claude(
                message
                    .reasoning_details
                    .as_ref()
                    .and_then(Option::as_deref)
                    .unwrap_or(&[]),
            );
            if let Some(Some(content)) = &message.content {
                match content {
                    chat::AssistantContent::Text(text) => blocks.push(text_block(text.clone())),
                    chat::AssistantContent::Parts(parts) => {
                        for part in parts {
                            match part {
                                chat::AssistantContentPart::Text(text) => {
                                    blocks.push(c::ContentBlock::Text(cache::claude_text(text)))
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
                            toolset_name: None,
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
                        report.omitted("tool_calls.custom", "call has no target representation");
                    }
                }
            }
            if let Some(Some(refusal)) = &message.refusal {
                blocks.push(text_block(refusal.clone()));
            }
            Ok(vec![c::Message {
                clear_at: None,
                output_config: None,
                role: c::Role::Assistant,
                content: c::MessageContent::Blocks(blocks),
                rest: Rest::new(),
            }])
        }
    }
}

fn claude_text(content: &c::MessageContent) -> Result<chat::TextContent, TransformError> {
    Ok(match content {
        c::MessageContent::Absent => chat::TextContent::Parts(Vec::new()),
        c::MessageContent::Text(text) => chat::TextContent::Text(text.clone()),
        c::MessageContent::Blocks(blocks) => {
            let parts = blocks
                .iter()
                .filter_map(|block| match block {
                    c::ContentBlock::Text(text) => Some(cache::chat_text(text)),
                    _ => None,
                })
                .collect();
            compact_chat(parts)
        }
    })
}

fn compact_chat(parts: Vec<chat::TextPart>) -> chat::TextContent {
    if parts
        .iter()
        .any(|part| part.prompt_cache_breakpoint.is_some())
    {
        chat::TextContent::Parts(parts)
    } else {
        chat::TextContent::Text(
            parts
                .into_iter()
                .map(|part| part.text)
                .collect::<Vec<_>>()
                .join(""),
        )
    }
}

pub(super) fn chat_text_blocks(content: &chat::TextContent) -> Vec<c::TextBlock> {
    match content {
        chat::TextContent::Parts(parts)
            if parts
                .iter()
                .any(|part| part.prompt_cache_breakpoint.is_some()) =>
        {
            parts.iter().map(cache::claude_text).collect()
        }
        _ => vec![c::TextBlock::builder(c::TextBlockType::Tag, chat_text(content)).build()],
    }
}

fn chat_content(content: &chat::TextContent) -> c::MessageContent {
    let blocks = chat_text_blocks(content);
    if blocks.iter().any(|block| block.cache_control.is_some()) {
        c::MessageContent::Blocks(blocks.into_iter().map(c::ContentBlock::Text).collect())
    } else {
        c::MessageContent::Text(chat_text(content))
    }
}

fn chat_result(content: &chat::TextContent) -> c::ToolResultContent {
    let blocks = chat_text_blocks(content);
    if blocks.iter().any(|block| block.cache_control.is_some()) {
        c::ToolResultContent::Blocks(
            blocks
                .into_iter()
                .map(c::ToolResultContentBlock::Text)
                .collect(),
        )
    } else {
        c::ToolResultContent::Text(chat_text(content))
    }
}

fn tool_result_text(
    block: &c::ToolResultBlock,
    report: &mut Report,
) -> Result<chat::TextContent, TransformError> {
    let mut parts = match &block.content {
        Some(c::ToolResultContent::Text(text)) => vec![chat::TextPart::builder(chat::TextPartType::Text, text.clone()).build()],
        Some(c::ToolResultContent::Blocks(blocks)) => blocks.iter().map(|block| match block {
            c::ToolResultContentBlock::Text(text) => Ok(cache::chat_text(text)),
            c::ToolResultContentBlock::ToolReference(reference) => {
                if reference.tool_name.trim().is_empty() {
                    return Err(TransformError::shape("tool_result.tool_reference.tool_name", "non-empty tool name required"));
                }
                report.changed("tool_result.tool_reference", "tool reference represented as text; callable schema remains in tools");
                let mut part = chat::TextPart::builder(chat::TextPartType::Text, serde_json::json!({"type":"tool_reference","tool_name":reference.tool_name}).to_string()).build();
                part.prompt_cache_breakpoint = reference.cache_control.as_ref().map(cache::claude_to_chat);
                Ok(part)
            }
            _ => {
                report.omitted("tool_result.content", "Chat tool messages have no non-text part");
                Ok(chat::TextPart::builder(chat::TextPartType::Text, String::new()).build())
            }
        }).filter_map(|value| crate::transform::optional(value).transpose()).collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    if parts.is_empty() {
        parts.push(chat::TextPart::builder(chat::TextPartType::Text, String::new()).build());
    }
    if block.is_error == Some(true) {
        report.changed(
            "tool_result.is_error",
            "error status is represented in the Chat tool result text",
        );
        parts[0].text.insert_str(0, "Tool error:\n");
    }
    if let Some(control) = &block.cache_control {
        parts
            .last_mut()
            .expect("text result has a part")
            .prompt_cache_breakpoint
            .get_or_insert_with(|| cache::claude_to_chat(control));
    }
    Ok(compact_chat(parts))
}

fn claude_blocks(
    content: &c::MessageContent,
) -> Result<Cow<'_, [c::ContentBlock]>, TransformError> {
    Ok(match content {
        c::MessageContent::Absent => Cow::Borrowed(&[]),
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
