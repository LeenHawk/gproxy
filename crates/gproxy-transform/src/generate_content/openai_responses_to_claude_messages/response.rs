use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;
use crate::common::usage;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    claude_to_responses(body)
}

pub(crate) fn claude_to_responses(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageResponseBody = serde_json::from_slice(&body)?;
    let id = input.id;
    let mut rest = input.rest;
    let created_at = take(&mut rest, "openai_created_at")?;
    let completed_at = take(&mut rest, "openai_completed_at")?;
    let mut output = Vec::new();
    let mut text = Vec::new();
    let mut parts = Vec::new();
    let mut message_id = None;
    for block in input.content {
        match block {
            claude::ResponseContentBlock::Text(mut block) => {
                message_id = take(&mut block.rest, "openai_item_id")?.or(message_id);
                text.push(block.text.clone());
                parts.push(openai::ResponseMessageOutputContentPart::OutputText(
                    openai::ResponseOutputText {
                        type_: openai::ResponseOutputTextType::OutputText,
                        annotations: Vec::new(),
                        logprobs: None,
                        text: block.text,
                        rest: block.rest,
                    },
                ));
            }
            claude::ResponseContentBlock::Thinking(mut block) => {
                flush_message(&mut output, &mut parts, &mut message_id);
                let item_id = take(&mut block.rest, "openai_item_id")?;
                output.push(reasoning(
                    item_id,
                    Some(block.thinking),
                    block.signature,
                    block.rest,
                ));
            }
            claude::ResponseContentBlock::RedactedThinking(mut block) => {
                flush_message(&mut output, &mut parts, &mut message_id);
                let item_id = take(&mut block.rest, "openai_item_id")?;
                output.push(reasoning(item_id, None, Some(block.data), block.rest));
            }
            claude::ResponseContentBlock::ToolUse(block) => {
                flush_message(&mut output, &mut parts, &mut message_id);
                let (item, _) = items::claude_call(
                    block.id,
                    block.input,
                    block.name,
                    block.rest,
                    openai::ResponseItemLifecycleStatus::Completed,
                )?;
                output.push(openai::ResponseItem::Typed(Box::new(item)));
            }
            claude::ResponseContentBlock::Compaction(mut block) => {
                flush_message(&mut output, &mut parts, &mut message_id);
                let item_id = take(&mut block.rest, "openai_item_id")?;
                output.push(openai::ResponseItem::Typed(Box::new(
                    openai::TypedResponseItem::Compaction {
                        encrypted_content: block.encrypted_content,
                        id: item_id,
                        created_by: None,
                        rest: block.rest,
                    },
                )));
            }
            claude::ResponseContentBlock::Raw(raw) => {
                flush_message(&mut output, &mut parts, &mut message_id);
                output.push(openai::ResponseItem::Unknown(raw));
            }
            other => {
                flush_message(&mut output, &mut parts, &mut message_id);
                output.push(openai::ResponseItem::Unknown(serde_json::to_value(other)?));
            }
        }
    }
    flush_message(&mut output, &mut parts, &mut message_id);
    let incomplete = matches!(
        input.stop_reason,
        claude::StopReason::Known(
            claude::StopReasonKnown::MaxTokens
                | claude::StopReasonKnown::ModelContextWindowExceeded
                | claude::StopReasonKnown::Refusal
        )
    );
    let response = openai::ResponseObject {
        id,
        created_at,
        background: None,
        completed_at,
        conversation: None,
        error: None,
        incomplete_details: incomplete.then_some(openai::IncompleteDetails {
            reason: Some(openai::IncompleteReason::MaxOutputTokens),
            rest: Default::default(),
        }),
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(crate::models::common::wire_string(&input.model)?.into()),
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output,
        output_text: (!text.is_empty()).then(|| text.join("")),
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: None,
        status: Some(if incomplete {
            openai::ResponseStatus::Incomplete
        } else {
            openai::ResponseStatus::Completed
        }),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: usage::claude_to_responses(input.usage),
        user: None,
        rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&response)?))
}

fn flush_message(
    output: &mut Vec<openai::ResponseItem>,
    parts: &mut Vec<openai::ResponseMessageOutputContentPart>,
    id: &mut Option<String>,
) {
    if parts.is_empty() {
        return;
    }
    output.push(openai::ResponseItem::Message(
        openai::ResponseMessageItem::Output(openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id: id.take(),
            role: openai::ResponseOutputMessageRole::Assistant,
            content: std::mem::take(parts),
            status: openai::ResponseItemLifecycleStatus::Completed,
            phase: None,
            rest: Default::default(),
        }),
    ));
}

fn reasoning(
    id: Option<String>,
    text: Option<String>,
    encrypted_content: Option<String>,
    rest: serde_json::Map<String, serde_json::Value>,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
        id,
        summary: Vec::new(),
        content: text.map(|text| {
            vec![openai::ResponseReasoningTextPart {
                type_: openai::ResponseReasoningTextType::ReasoningText,
                text,
                rest: Default::default(),
            }]
        }),
        encrypted_content,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        rest,
    }))
}

fn take<T: serde::de::DeserializeOwned>(
    rest: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<Option<T>, TransformError> {
    rest.remove(name)
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}
