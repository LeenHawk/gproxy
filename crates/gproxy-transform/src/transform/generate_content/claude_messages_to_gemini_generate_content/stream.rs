use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: claude::StreamEvent,
    ctx: &TransformContext,
) -> Result<gemini::StreamGenerateContentChunk, TransformError> {
    let _ = ctx;
    Ok(match input {
        claude::StreamEvent::Known(event) => known_event_to_gemini(*event),
        claude::StreamEvent::Unknown(_) => empty_chunk(),
    })
}

fn known_event_to_gemini(event: claude::KnownStreamEvent) -> gemini::GenerateContentResponse {
    match event {
        claude::KnownStreamEvent::MessageStart { message, .. } => {
            let mut chunk = empty_chunk_with_usage(Some(
                common::completion_usage_to_gemini(Some(common::claude_usage_to_completion(
                    message.usage,
                )))
                .unwrap_or_default(),
            ));
            chunk.response_id = Some(message.id);
            chunk.model_version = Some(common::claude_model_string(message.model));
            chunk
        }
        claude::KnownStreamEvent::ContentBlockStart { content_block, .. } => {
            content_block_to_gemini(*content_block)
        }
        claude::KnownStreamEvent::ContentBlockDelta { delta, .. } => event_delta_to_gemini(*delta),
        claude::KnownStreamEvent::MessageDelta { delta, usage, .. } => {
            let delta = *delta;
            let finish_reason = delta.stop_reason.map(claude_stop_to_gemini_finish);
            candidate_chunk(
                None,
                finish_reason,
                common::completion_usage_to_gemini(
                    usage.map(|usage| common::claude_usage_to_completion(*usage)),
                ),
            )
        }
        claude::KnownStreamEvent::Error { .. } => candidate_chunk(
            None,
            Some(gemini::FinishReason::Known(
                gemini::FinishReasonKnown::Safety,
            )),
            None,
        ),
        _ => empty_chunk(),
    }
}

fn content_block_to_gemini(block: claude::ContentBlock) -> gemini::GenerateContentResponse {
    match block {
        claude::ContentBlock::Text(block) => text_chunk(block.text, false),
        claude::ContentBlock::Thinking(block) => text_chunk(block.thinking, true),
        claude::ContentBlock::ToolUse(block) => {
            function_call_chunk(Some(block.id), block.name, Some(block.input))
        }
        claude::ContentBlock::McpToolUse(block) => {
            function_call_chunk(Some(block.id), block.name, Some(block.input))
        }
        _ => empty_chunk(),
    }
}

fn event_delta_to_gemini(delta: claude::EventDelta) -> gemini::GenerateContentResponse {
    match delta {
        claude::EventDelta::Known(delta) => match *delta {
            claude::KnownEventDelta::Text { text, .. } => text_chunk(text, false),
            claude::KnownEventDelta::Thinking { thinking, .. } => text_chunk(thinking, true),
            claude::KnownEventDelta::Compaction { content, .. } => text_chunk(content, false),
            _ => empty_chunk(),
        },
        claude::EventDelta::Unknown(_) => empty_chunk(),
    }
}

fn text_chunk(text: String, thought: bool) -> gemini::GenerateContentResponse {
    candidate_chunk(
        Some(gemini::Content {
            parts: vec![gemini::Part {
                thought: thought.then_some(true),
                data: Some(gemini::PartData::Text { text }),
                ..Default::default()
            }],
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            extra: Default::default(),
        }),
        None,
        None,
    )
}

fn function_call_chunk(
    id: Option<String>,
    name: String,
    args: Option<claude::JsonObject>,
) -> gemini::GenerateContentResponse {
    candidate_chunk(
        Some(gemini::Content {
            parts: vec![gemini::Part {
                data: Some(gemini::PartData::FunctionCall {
                    function_call: gemini::FunctionCall {
                        id,
                        name,
                        args,
                        extra: Default::default(),
                    },
                }),
                ..Default::default()
            }],
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            extra: Default::default(),
        }),
        None,
        None,
    )
}

fn candidate_chunk(
    content: Option<gemini::Content>,
    finish_reason: Option<gemini::FinishReason>,
    usage_metadata: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    gemini::GenerateContentResponse {
        candidates: vec![gemini::Candidate {
            content,
            finish_reason,
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
            extra: Default::default(),
        }],
        prompt_feedback: None,
        usage_metadata,
        model_version: None,
        response_id: None,
        model_status: None,
        extra: Default::default(),
    }
}

fn empty_chunk_with_usage(
    usage_metadata: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    gemini::GenerateContentResponse {
        candidates: Vec::new(),
        prompt_feedback: None,
        usage_metadata,
        model_version: None,
        response_id: None,
        model_status: None,
        extra: Default::default(),
    }
}

fn empty_chunk() -> gemini::GenerateContentResponse {
    empty_chunk_with_usage(None)
}

fn claude_stop_to_gemini_finish(reason: claude::StopReason) -> gemini::FinishReason {
    let known = match reason {
        claude::StopReason::Known(
            claude::StopReasonKnown::MaxTokens
            | claude::StopReasonKnown::ModelContextWindowExceeded,
        ) => gemini::FinishReasonKnown::MaxTokens,
        claude::StopReason::Known(claude::StopReasonKnown::Refusal) => {
            gemini::FinishReasonKnown::Safety
        }
        _ => gemini::FinishReasonKnown::Stop,
    };
    gemini::FinishReason::Known(known)
}
