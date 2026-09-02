use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;
use crate::common::usage;

mod helpers;
use helpers::*;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    claude_to_responses(body)
}

pub(crate) fn claude_to_responses(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageResponseBody = serde_json::from_slice(&body)?;
    let id = input.id;
    let mut rest = input.rest;
    crate::common::claude_message_controls::preserve_input_transformations(
        &mut rest,
        input.input_transformations,
    )?;
    let created_at = take(&mut rest, "openai_created_at")?;
    let completed_at = take(&mut rest, "openai_completed_at")?;
    let service_tier = claude_service_tier(&input.usage)?;
    let mut output = Vec::new();
    let mut text = Vec::new();
    let mut parts = Vec::new();
    let mut message_id = None;
    let mut message_index = 0;
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
                let item_id = take(&mut block.rest, "openai_item_id")?;
                output.push(reasoning(
                    item_id,
                    Some(block.thinking),
                    block.signature,
                    block.rest,
                ));
            }
            claude::ResponseContentBlock::RedactedThinking(mut block) => {
                let item_id = take(&mut block.rest, "openai_item_id")?;
                output.push(reasoning(item_id, None, Some(block.data), block.rest));
            }
            claude::ResponseContentBlock::ToolUse(block) => {
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
                output.push(openai::ResponseItem::Unknown(raw));
            }
            other => {
                output.push(openai::ResponseItem::Unknown(serde_json::to_value(other)?));
            }
        }
    }
    flush_message(
        &mut output,
        &mut parts,
        &mut message_id,
        &id,
        &mut message_index,
    );
    let stop_reason = crate::models::common::wire_string(&input.stop_reason)?;
    let incomplete_reason = match stop_reason.as_str() {
        "max_tokens" | "model_context_window_exceeded" => {
            Some(openai::IncompleteReason::MaxOutputTokens)
        }
        "refusal" => Some(openai::IncompleteReason::ContentFilter),
        "end_turn" | "stop_sequence" | "tool_use" | "pause_turn" | "compaction" => None,
        _ => None,
    };
    let response = openai::ResponseObject {
        id,
        created_at,
        background: None,
        completed_at,
        conversation: None,
        error: None,
        incomplete_details: incomplete_reason
            .clone()
            .map(|reason| openai::IncompleteDetails {
                reason: Some(reason),
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
        service_tier,
        status: Some(if incomplete_reason.is_some() {
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

fn claude_service_tier(
    usage: &claude::Usage,
) -> Result<Option<openai::ServiceTier>, TransformError> {
    if matches!(
        usage.speed,
        Some(claude::Speed::Known(claude::SpeedKnown::Fast))
    ) {
        return Ok(Some(openai::ServiceTier::Priority));
    }
    let Some(tier) = usage.service_tier.as_ref() else {
        return Ok(None);
    };
    let tier = crate::models::common::wire_string(tier)?;
    Ok(Some(if tier == "priority" {
        openai::ServiceTier::Priority
    } else {
        openai::ServiceTier::Default
    }))
}
