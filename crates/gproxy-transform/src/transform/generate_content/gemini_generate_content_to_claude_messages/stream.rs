use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: gemini::StreamGenerateContentChunk,
    ctx: &TransformContext,
) -> Result<claude::StreamEvent, TransformError> {
    let _ = ctx;
    Ok(gemini_chunk_to_claude(input))
}

fn gemini_chunk_to_claude(input: gemini::GenerateContentResponse) -> claude::StreamEvent {
    let usage = input.usage_metadata.map(|usage| {
        let service_tier = common::gemini_usage_service_tier_to_claude(usage.service_tier.clone());
        let mut usage =
            common::completion_usage_to_claude(Some(common::gemini_usage_to_completion(usage)));
        usage.service_tier = service_tier;
        usage
    });
    let blocked = input
        .prompt_feedback
        .as_ref()
        .and_then(|feedback| feedback.block_reason.as_ref())
        .is_some();

    if input.candidates.is_empty() {
        return if blocked {
            message_delta(
                Some(claude::StopReason::Known(claude::StopReasonKnown::Refusal)),
                usage,
            )
        } else if usage.is_some() {
            message_delta(None, usage)
        } else {
            ping()
        };
    }

    let mut candidates = input.candidates.into_iter();
    let Some(candidate) = candidates.next() else {
        return ping();
    };
    let index = candidate.index.map(index_to_u64).unwrap_or_default();

    if let Some(content) = candidate.content
        && let Some(event) = gemini_content_to_claude(content, index)
    {
        return event;
    }

    if let Some(finish_reason) = candidate.finish_reason {
        return message_delta(Some(gemini_finish_to_claude_stop(finish_reason)), usage);
    }

    ping()
}

fn gemini_content_to_claude(content: gemini::Content, index: u64) -> Option<claude::StreamEvent> {
    content
        .parts
        .into_iter()
        .find_map(|part| part_to_claude(part, index))
}

fn part_to_claude(part: gemini::Part, index: u64) -> Option<claude::StreamEvent> {
    match part.data? {
        gemini::PartData::Text { text } => {
            if part.thought.unwrap_or(false) {
                Some(content_delta(index, thinking_delta(text)))
            } else {
                Some(content_delta(index, text_delta(text)))
            }
        }
        gemini::PartData::FunctionCall { function_call } => {
            Some(known(claude::KnownStreamEvent::ContentBlockStart {
                index,
                content_block: Box::new(claude::ContentBlock::ToolUse(
                    claude::ResponseToolUseBlock {
                        id: function_call.id.unwrap_or_else(|| format!("call_{index}")),
                        input: function_call.args.unwrap_or_default(),
                        name: function_call.name,
                        type_: claude::ToolUseBlockType::ToolUse,
                        caller: None,
                        extra: Default::default(),
                    },
                )),
                extra: Default::default(),
            }))
        }
        _ => None,
    }
}

fn content_delta(index: u64, delta: claude::KnownEventDelta) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::ContentBlockDelta {
        index,
        delta: Box::new(claude::EventDelta::Known(Box::new(delta))),
        extra: Default::default(),
    })
}

fn text_delta(text: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::Text {
        text,
        extra: Default::default(),
    }
}

fn thinking_delta(thinking: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::Thinking {
        estimated_tokens: None,
        thinking,
        extra: Default::default(),
    }
}

fn message_delta(
    stop_reason: Option<claude::StopReason>,
    usage: Option<claude::Usage>,
) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::MessageDelta {
        context_management: None,
        delta: Box::new(claude::MessageDelta {
            container: None,
            stop_reason,
            stop_sequence: None,
            stop_details: None,
            extra: Default::default(),
        }),
        usage: usage.map(Box::new),
        extra: Default::default(),
    })
}

fn gemini_finish_to_claude_stop(reason: gemini::FinishReason) -> claude::StopReason {
    match reason {
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::Safety
            | gemini::FinishReasonKnown::Recitation
            | gemini::FinishReasonKnown::Blocklist
            | gemini::FinishReasonKnown::ProhibitedContent
            | gemini::FinishReasonKnown::Spii
            | gemini::FinishReasonKnown::ImageSafety
            | gemini::FinishReasonKnown::ImageProhibitedContent,
        ) => claude::StopReason::Known(claude::StopReasonKnown::Refusal),
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::UnexpectedToolCall
            | gemini::FinishReasonKnown::TooManyToolCalls
            | gemini::FinishReasonKnown::MalformedFunctionCall,
        ) => claude::StopReason::Known(claude::StopReasonKnown::ToolUse),
        _ => claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
    }
}

fn index_to_u64(index: i32) -> u64 {
    u64::try_from(index).unwrap_or_default()
}

fn ping() -> claude::StreamEvent {
    known(claude::KnownStreamEvent::Ping {
        extra: Default::default(),
    })
}

fn known(event: claude::KnownStreamEvent) -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(event))
}
