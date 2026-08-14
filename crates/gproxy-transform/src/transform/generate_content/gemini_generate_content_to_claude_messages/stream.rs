use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::response::gemini_usage_to_claude;

pub fn stream_event(
    input: gemini::StreamGenerateContentChunk,
    ctx: &TransformContext,
) -> Result<Vec<claude::StreamEvent>, TransformError> {
    StreamTransform::default().push(input, ctx)
}

#[derive(Default)]
pub struct StreamTransform {
    lifecycle: common::ClaudeStreamLifecycle,
}

impl StreamTransform {
    pub fn push(
        &mut self,
        input: gemini::StreamGenerateContentChunk,
        _: &TransformContext,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let fallback_start = common::claude_message_start(
            input
                .response_id
                .clone()
                .unwrap_or_else(|| "gemini_message".to_owned()),
            input
                .model_version
                .clone()
                .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned()),
            input
                .usage_metadata
                .clone()
                .map(gemini_usage_to_claude)
                .unwrap_or_else(common::empty_claude_usage),
        );
        let events = gemini_chunk_to_claude(input);
        Ok(self.lifecycle.push(events, fallback_start))
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        Ok(self.lifecycle.finish())
    }
}

fn gemini_chunk_to_claude(input: gemini::GenerateContentResponse) -> Vec<claude::StreamEvent> {
    let usage = input.usage_metadata.map(gemini_usage_to_claude);
    let blocked = input
        .prompt_feedback
        .as_ref()
        .and_then(|feedback| feedback.block_reason.as_ref())
        .is_some();

    if input.candidates.is_empty() {
        return if blocked {
            vec![message_delta(
                Some(claude::StopReason::Known(claude::StopReasonKnown::Refusal)),
                usage,
            )]
        } else if usage.is_some() {
            vec![message_delta(None, usage)]
        } else {
            Vec::new()
        };
    }

    let candidate_count = input.candidates.len();
    let mut out = Vec::new();
    for (fallback_index, candidate) in input.candidates.into_iter().enumerate() {
        let index = candidate
            .index
            .map(index_to_u64)
            .unwrap_or_else(|| u64::try_from(fallback_index).unwrap_or_default());
        if let Some(content) = candidate.content {
            out.extend(gemini_content_to_claude(content, index));
        }
        if let Some(finish_reason) = candidate.finish_reason {
            out.push(message_delta(
                Some(gemini_finish_to_claude_stop(finish_reason)),
                (candidate_count == 1).then(|| usage.clone()).flatten(),
            ));
        }
    }
    out
}

fn gemini_content_to_claude(content: gemini::Content, index: u64) -> Vec<claude::StreamEvent> {
    content
        .parts
        .into_iter()
        .flat_map(|part| part_to_claude(part, index))
        .collect()
}

fn part_to_claude(part: gemini::Part, index: u64) -> Vec<claude::StreamEvent> {
    let signature = part.thought_signature;
    let Some(data) = part.data else {
        return signature
            .map(|signature| vec![content_delta(index, signature_delta(signature))])
            .unwrap_or_default();
    };
    match data {
        gemini::PartData::Text { text } => {
            if part.thought.unwrap_or(false) {
                let mut events = vec![content_delta(index, thinking_delta(text))];
                if let Some(signature) = signature {
                    events.push(content_delta(index, signature_delta(signature)));
                }
                events
            } else {
                vec![content_delta(index, text_delta(text))]
            }
        }
        gemini::PartData::FunctionCall { function_call } => {
            vec![known(claude::KnownStreamEvent::ContentBlockStart {
                index,
                content_block: Box::new(claude::ContentBlock::ToolUse(crate::protocol::wire!(
                    claude::ResponseToolUseBlock {
                        id: function_call.id.unwrap_or_else(|| format!("call_{index}")),
                        input: function_call.args.unwrap_or_default(),
                        name: function_call.name,
                        type_: claude::ToolUseBlockType::ToolUse,
                        caller: None,
                        extra: Default::default(),
                    }
                ))),
                extra: Default::default(),
            })]
        }
        _ => Vec::new(),
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

fn signature_delta(signature: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::Signature {
        signature,
        extra: Default::default(),
    }
}

fn message_delta(
    stop_reason: Option<claude::StopReason>,
    usage: Option<claude::Usage>,
) -> claude::StreamEvent {
    known(crate::protocol::wire!(
        claude::KnownStreamEvent::MessageDelta {
            context_management: None,
            delta: Box::new(crate::protocol::wire!(claude::MessageDelta {
                container: None,
                stop_reason,
                stop_sequence: None,
                stop_details: None,
                extra: Default::default(),
            })),
            usage: usage.map(Box::new),
            extra: Default::default(),
        }
    ))
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

fn known(event: claude::KnownStreamEvent) -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(event))
}
