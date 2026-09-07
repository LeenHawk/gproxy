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
    let response = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&response)?))
}

pub(crate) fn transform_typed(
    input: claude::CreateMessageResponseBody,
) -> Result<openai::ResponseObject, TransformError> {
    let id = input.id;
    let service_tier = claude_service_tier(&input.usage)?;
    let refusal =
        crate::common::stop::refusal_text(&input.stop_reason, input.stop_details.as_ref());
    let mut output = Vec::new();
    let mut text = Vec::new();
    let mut parts = Vec::new();
    let mut message_id = None;
    let mut message_index = 0;
    for block in input.content {
        match block {
            claude::ResponseContentBlock::Text(block) => {
                text.push(block.text.clone());
                parts.push(openai::ResponseMessageOutputContentPart::OutputText(
                    crate::wire!(openai::ResponseOutputText {
                        type_: openai::ResponseOutputTextType::OutputText,
                        annotations: Vec::new(),
                        logprobs: None,
                        text: block.text,
                        rest: Default::default(),
                    }),
                ));
            }
            claude::ResponseContentBlock::Thinking(block) => {
                output.push(reasoning(
                    None,
                    Some(block.thinking),
                    block.signature,
                    Default::default(),
                ));
            }
            claude::ResponseContentBlock::RedactedThinking(block) => {
                output.push(reasoning(None, None, Some(block.data), Default::default()));
            }
            claude::ResponseContentBlock::ToolUse(block) => {
                let (item, _) = items::claude_call(
                    block.id,
                    block.input,
                    block.name,
                    openai::ResponseItemLifecycleStatus::Completed,
                )?;
                output.push(openai::ResponseItem::Typed(Box::new(item)));
            }
            claude::ResponseContentBlock::Compaction(block) => {
                output.push(openai::ResponseItem::Typed(Box::new(
                    openai::TypedResponseItem::Compaction {
                        encrypted_content: block.encrypted_content,
                        id: None,
                        created_by: None,
                        rest: Default::default(),
                    },
                )));
            }
            claude::ResponseContentBlock::Raw(_) => {}
            _ => {}
        }
    }
    if let Some(refusal) = refusal {
        parts.push(openai::ResponseMessageOutputContentPart::Refusal(
            crate::wire!(openai::ResponseRefusal {
                type_: openai::ResponseRefusalType::Refusal,
                refusal,
                rest: Default::default(),
            }),
        ));
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
    let response = crate::wire!(openai::ResponseObject {
        id,
        created_at: None,
        background: None,
        completed_at: None,
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
        rest: Default::default(),
    });
    Ok(response)
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
