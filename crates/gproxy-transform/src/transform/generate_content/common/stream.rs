use std::collections::BTreeMap;

use crate::protocol::{claude, gemini, openai};

use super::scalar::{i32_to_u32, u32_to_i32};

pub(in crate::transform::generate_content) fn default_openai_model() -> openai::OpenAiModelId {
    super::DEFAULT_OPENAI_MODEL.to_owned().into()
}

pub(in crate::transform::generate_content) fn claude_message_start(
    id: String,
    model: String,
    usage: claude::Usage,
) -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::MessageStart {
        message: Box::new(crate::protocol::wire!(claude::CreateMessageStartBody {
            id,
            type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
            role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
            content: Vec::new(),
            model: model.into(),
            stop_reason: None,
            stop_sequence: None,
            usage,
            extra: Default::default(),
        })),
        extra: Default::default(),
    }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClaudeBlockKind {
    Text,
    Thinking,
    Tool,
    Compaction,
}

/// Enforce the ordering required by the Claude Messages streaming protocol.
#[derive(Default)]
pub(in crate::transform::generate_content) struct ClaudeStreamLifecycle {
    message_started: bool,
    open_blocks: BTreeMap<u64, (ClaudeBlockKind, u64)>,
    next_block_index: u64,
    terminated: bool,
}

impl ClaudeStreamLifecycle {
    pub(in crate::transform::generate_content) fn push(
        &mut self,
        events: Vec<claude::StreamEvent>,
        fallback_start: claude::StreamEvent,
    ) -> Vec<claude::StreamEvent> {
        let mut out = Vec::new();
        let mut fallback_start = Some(fallback_start);

        for event in events {
            if self.terminated {
                break;
            }
            match event {
                claude::StreamEvent::Known(mut known) => match known.as_mut() {
                    claude::KnownStreamEvent::MessageStart { .. } => {
                        if !self.message_started {
                            self.message_started = true;
                            out.push(claude::StreamEvent::Known(known));
                        }
                    }
                    claude::KnownStreamEvent::ContentBlockStart {
                        index,
                        content_block,
                        ..
                    } => {
                        self.ensure_message_start(&mut out, &mut fallback_start);
                        self.close_blocks(&mut out);
                        if let Some(kind) = claude_block_kind(content_block) {
                            let source_index = *index;
                            let output_index = self.allocate_block_index();
                            self.open_blocks.insert(source_index, (kind, output_index));
                            *index = output_index;
                        }
                        out.push(claude::StreamEvent::Known(known));
                    }
                    claude::KnownStreamEvent::ContentBlockDelta { index, delta, .. } => {
                        self.ensure_message_start(&mut out, &mut fallback_start);
                        let source_index = *index;
                        let kind = claude_delta_kind(delta).unwrap_or(ClaudeBlockKind::Text);
                        let output_index = if let Some((_, output_index)) = self
                            .open_blocks
                            .get(&source_index)
                            .filter(|(open_kind, _)| *open_kind == kind)
                        {
                            *output_index
                        } else {
                            self.close_blocks(&mut out);
                            let output_index = self.allocate_block_index();
                            out.push(claude_block_start(output_index, kind));
                            self.open_blocks.insert(source_index, (kind, output_index));
                            output_index
                        };
                        *index = output_index;
                        out.push(claude::StreamEvent::Known(known));
                    }
                    claude::KnownStreamEvent::ContentBlockStop { index, .. } => {
                        let source_index = *index;
                        if let Some((_, output_index)) = self.open_blocks.remove(&source_index) {
                            *index = output_index;
                            out.push(claude::StreamEvent::Known(known));
                        }
                    }
                    claude::KnownStreamEvent::MessageDelta { delta, .. } => {
                        self.ensure_message_start(&mut out, &mut fallback_start);
                        self.close_blocks(&mut out);
                        let terminal = delta.stop_reason.is_some();
                        out.push(claude::StreamEvent::Known(known));
                        if terminal {
                            out.push(claude_message_stop());
                            self.terminated = true;
                        }
                    }
                    claude::KnownStreamEvent::MessageStop { .. } => {
                        self.ensure_message_start(&mut out, &mut fallback_start);
                        self.close_blocks(&mut out);
                        out.push(claude::StreamEvent::Known(known));
                        self.terminated = true;
                    }
                    claude::KnownStreamEvent::Error { .. } => {
                        self.close_blocks(&mut out);
                        out.push(claude::StreamEvent::Known(known));
                        self.terminated = true;
                    }
                    claude::KnownStreamEvent::Ping { .. } => {
                        out.push(claude::StreamEvent::Known(known));
                    }
                    _ => unreachable!(
                        "new non-exhaustive protocol variant requires a lockstep transform update"
                    ),
                },
                claude::StreamEvent::Unknown(event) => {
                    out.push(claude::StreamEvent::Unknown(event));
                }
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            }
        }
        out
    }

    pub(in crate::transform::generate_content) fn finish(&mut self) -> Vec<claude::StreamEvent> {
        if !self.message_started || self.terminated {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.close_blocks(&mut out);
        out.push(claude_message_stop());
        self.terminated = true;
        out
    }

    fn ensure_message_start(
        &mut self,
        out: &mut Vec<claude::StreamEvent>,
        fallback_start: &mut Option<claude::StreamEvent>,
    ) {
        if !self.message_started {
            out.push(fallback_start.take().unwrap_or_else(|| {
                claude_message_start(
                    "message".to_owned(),
                    super::DEFAULT_OPENAI_MODEL.to_owned(),
                    super::empty_claude_usage(),
                )
            }));
            self.message_started = true;
        }
    }

    fn close_blocks(&mut self, out: &mut Vec<claude::StreamEvent>) {
        for (_, (_, output_index)) in std::mem::take(&mut self.open_blocks) {
            out.push(claude_content_block_stop(output_index));
        }
    }

    fn allocate_block_index(&mut self) -> u64 {
        let index = self.next_block_index;
        self.next_block_index = self.next_block_index.saturating_add(1);
        index
    }
}

fn claude_block_kind(block: &claude::ContentBlock) -> Option<ClaudeBlockKind> {
    match block {
        claude::ContentBlock::Text(_) => Some(ClaudeBlockKind::Text),
        claude::ContentBlock::Thinking(_) => Some(ClaudeBlockKind::Thinking),
        claude::ContentBlock::ToolUse(_) | claude::ContentBlock::McpToolUse(_) => {
            Some(ClaudeBlockKind::Tool)
        }
        claude::ContentBlock::Compaction(_) => Some(ClaudeBlockKind::Compaction),
        _ => None,
    }
}

fn claude_delta_kind(delta: &claude::EventDelta) -> Option<ClaudeBlockKind> {
    let claude::EventDelta::Known(delta) = delta else {
        return None;
    };
    match delta.as_ref() {
        claude::KnownEventDelta::Text { .. } | claude::KnownEventDelta::Citations { .. } => {
            Some(ClaudeBlockKind::Text)
        }
        claude::KnownEventDelta::Thinking { .. } | claude::KnownEventDelta::Signature { .. } => {
            Some(ClaudeBlockKind::Thinking)
        }
        claude::KnownEventDelta::InputJson { .. } => Some(ClaudeBlockKind::Tool),
        claude::KnownEventDelta::Compaction { .. } => Some(ClaudeBlockKind::Compaction),
        _ => None,
    }
}

fn claude_block_start(index: u64, kind: ClaudeBlockKind) -> claude::StreamEvent {
    let content_block = match kind {
        ClaudeBlockKind::Text => {
            claude::ContentBlock::Text(crate::protocol::wire!(claude::ResponseTextBlock {
                citations: None,
                text: String::new(),
                type_: claude::TextBlockType::Text,
                extra: Default::default(),
            }))
        }
        ClaudeBlockKind::Thinking => {
            claude::ContentBlock::Thinking(crate::protocol::wire!(claude::ThinkingBlock {
                signature: String::new(),
                thinking: String::new(),
                type_: claude::ThinkingBlockType::Thinking,
            }))
        }
        ClaudeBlockKind::Tool => {
            claude::ContentBlock::ToolUse(crate::protocol::wire!(claude::ResponseToolUseBlock {
                id: format!("call_{index}"),
                input: Default::default(),
                name: "unknown".to_owned(),
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            }))
        }
        ClaudeBlockKind::Compaction => claude::ContentBlock::Compaction(crate::protocol::wire!(
            claude::ResponseCompactionBlock {
                content: None,
                encrypted_content: String::new(),
                type_: claude::CompactionBlockType::Compaction,
                extra: Default::default(),
            }
        )),
    };
    claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::ContentBlockStart {
        index,
        content_block: Box::new(content_block),
        extra: Default::default(),
    }))
}

fn claude_content_block_stop(index: u64) -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::ContentBlockStop {
        index,
        extra: Default::default(),
    }))
}

fn claude_message_stop() -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::MessageStop {
        extra: Default::default(),
    }))
}

pub(in crate::transform::generate_content) fn empty_chat_delta() -> openai::ChatDelta {
    crate::protocol::wire!(openai::ChatDelta {
        role: None,
        content: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: None,
        function_call: None,
        obfuscation: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn chat_delta_chunk(
    id: String,
    model: openai::OpenAiModelId,
    created: u64,
    index: u32,
    delta: openai::ChatDelta,
    finish_reason: Option<openai::ChatFinishReason>,
    usage: Option<openai::CompletionUsage>,
) -> openai::ChatCompletionChunk {
    crate::protocol::wire!(openai::ChatCompletionChunk {
        id,
        choices: vec![crate::protocol::wire!(openai::ChatChunkChoice {
            index,
            delta,
            finish_reason,
            logprobs: None,
            extra: Default::default(),
        })],
        created,
        model,
        object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
        service_tier: None,
        system_fingerprint: None,
        usage,
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn empty_chat_chunk(
    id: String,
    model: openai::OpenAiModelId,
    created: u64,
    usage: Option<openai::CompletionUsage>,
) -> openai::ChatCompletionChunk {
    crate::protocol::wire!(openai::ChatCompletionChunk {
        id,
        choices: Vec::new(),
        created,
        model,
        object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
        service_tier: None,
        system_fingerprint: None,
        usage,
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn chat_text_delta(
    id: String,
    model: openai::OpenAiModelId,
    created: u64,
    index: u32,
    text: String,
) -> openai::ChatCompletionChunk {
    let mut delta = empty_chat_delta();
    delta.content = Some(text);
    chat_delta_chunk(id, model, created, index, delta, None, None)
}

pub(in crate::transform::generate_content) fn chat_reasoning_delta(
    id: String,
    model: openai::OpenAiModelId,
    created: u64,
    index: u32,
    text: String,
) -> openai::ChatCompletionChunk {
    let mut delta = empty_chat_delta();
    delta.reasoning_content = Some(text);
    chat_delta_chunk(id, model, created, index, delta, None, None)
}

pub(in crate::transform::generate_content) fn chat_refusal_delta(
    id: String,
    model: openai::OpenAiModelId,
    created: u64,
    index: u32,
    text: String,
) -> openai::ChatCompletionChunk {
    let mut delta = empty_chat_delta();
    delta.refusal = Some(text);
    chat_delta_chunk(id, model, created, index, delta, None, None)
}

pub(in crate::transform::generate_content) fn chat_finish_chunk(
    id: String,
    model: openai::OpenAiModelId,
    created: u64,
    finish_reason: openai::ChatFinishReason,
    usage: Option<openai::CompletionUsage>,
) -> openai::ChatCompletionChunk {
    chat_delta_chunk(
        id,
        model,
        created,
        0,
        empty_chat_delta(),
        Some(finish_reason),
        usage,
    )
}

pub(in crate::transform::generate_content) fn chat_function_tool_delta(
    index: u32,
    id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
) -> openai::ChatToolCallDelta {
    crate::protocol::wire!(openai::ChatToolCallDelta {
        index,
        id,
        type_: Some(openai::ChatToolCallType::Function),
        function: Some(crate::protocol::wire!(openai::FunctionCallDelta {
            arguments,
            name,
            extra: Default::default(),
        })),
        custom: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn chat_custom_tool_delta(
    index: u32,
    id: Option<String>,
    name: Option<String>,
    input: Option<String>,
) -> openai::ChatToolCallDelta {
    crate::protocol::wire!(openai::ChatToolCallDelta {
        index,
        id,
        type_: Some(openai::ChatToolCallType::Custom),
        function: None,
        custom: Some(crate::protocol::wire!(openai::CustomToolCallDelta {
            input,
            name,
            extra: Default::default(),
        })),
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn chat_finish_reason_to_claude(
    reason: openai::ChatFinishReason,
) -> claude::StopReason {
    match reason {
        openai::ChatFinishReason::Stop => {
            claude::StopReason::Known(claude::StopReasonKnown::EndTurn)
        }
        openai::ChatFinishReason::Length => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        openai::ChatFinishReason::ToolCalls | openai::ChatFinishReason::FunctionCall => {
            claude::StopReason::Known(claude::StopReasonKnown::ToolUse)
        }
        openai::ChatFinishReason::ContentFilter => {
            claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(in crate::transform::generate_content) fn claude_stop_reason_to_chat(
    reason: claude::StopReason,
) -> openai::ChatFinishReason {
    match reason {
        claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        | claude::StopReason::Known(claude::StopReasonKnown::ModelContextWindowExceeded) => {
            openai::ChatFinishReason::Length
        }
        claude::StopReason::Known(claude::StopReasonKnown::ToolUse) => {
            openai::ChatFinishReason::ToolCalls
        }
        claude::StopReason::Known(claude::StopReasonKnown::Refusal) => {
            openai::ChatFinishReason::ContentFilter
        }
        claude::StopReason::Known(
            claude::StopReasonKnown::EndTurn
            | claude::StopReasonKnown::StopSequence
            | claude::StopReasonKnown::PauseTurn
            | claude::StopReasonKnown::Compaction,
        )
        | claude::StopReason::Unknown(_) => openai::ChatFinishReason::Stop,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(in crate::transform::generate_content) fn chat_finish_reason_to_gemini(
    reason: openai::ChatFinishReason,
) -> gemini::FinishReason {
    let known = match reason {
        openai::ChatFinishReason::Stop => gemini::FinishReasonKnown::Stop,
        openai::ChatFinishReason::Length => gemini::FinishReasonKnown::MaxTokens,
        openai::ChatFinishReason::ToolCalls | openai::ChatFinishReason::FunctionCall => {
            gemini::FinishReasonKnown::Stop
        }
        openai::ChatFinishReason::ContentFilter => gemini::FinishReasonKnown::Safety,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    gemini::FinishReason::Known(known)
}

pub(in crate::transform::generate_content) fn gemini_finish_reason_to_chat(
    reason: gemini::FinishReason,
) -> openai::ChatFinishReason {
    match reason {
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => {
            openai::ChatFinishReason::Length
        }
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::Safety
            | gemini::FinishReasonKnown::Recitation
            | gemini::FinishReasonKnown::Blocklist
            | gemini::FinishReasonKnown::ProhibitedContent
            | gemini::FinishReasonKnown::Spii
            | gemini::FinishReasonKnown::ImageSafety
            | gemini::FinishReasonKnown::ImageProhibitedContent,
        ) => openai::ChatFinishReason::ContentFilter,
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::UnexpectedToolCall
            | gemini::FinishReasonKnown::TooManyToolCalls
            | gemini::FinishReasonKnown::MalformedFunctionCall,
        ) => openai::ChatFinishReason::ToolCalls,
        gemini::FinishReason::Known(_) | gemini::FinishReason::Unknown(_) => {
            openai::ChatFinishReason::Stop
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(in crate::transform::generate_content) fn completion_usage_to_gemini(
    usage: Option<openai::CompletionUsage>,
) -> Option<gemini::UsageMetadata> {
    let usage = usage?;
    let thoughts = usage
        .completion_tokens_details
        .and_then(|details| details.reasoning_tokens)
        .map(u32_to_i32);
    Some(crate::protocol::wire!(gemini::UsageMetadata {
        prompt_token_count: Some(u32_to_i32(usage.prompt_tokens)),
        cached_content_token_count: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .map(u32_to_i32),
        candidates_token_count: Some(
            u32_to_i32(usage.completion_tokens).saturating_sub(thoughts.unwrap_or_default()),
        ),
        thoughts_token_count: thoughts,
        total_token_count: Some(u32_to_i32(usage.total_tokens)),
        tool_use_prompt_token_count: None,
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: None,
        extra: Default::default(),
    }))
}

pub(in crate::transform::generate_content) fn gemini_usage_to_completion(
    usage: gemini::UsageMetadata,
) -> openai::CompletionUsage {
    let prompt_tokens = usage.prompt_token_count.map(i32_to_u32).unwrap_or_default();
    let thoughts = usage.thoughts_token_count.map(i32_to_u32);
    let completion_tokens = usage
        .candidates_token_count
        .map(i32_to_u32)
        .unwrap_or_default()
        .saturating_add(thoughts.unwrap_or_default());
    let total_tokens = usage
        .total_token_count
        .map(i32_to_u32)
        .unwrap_or_else(|| prompt_tokens.saturating_add(completion_tokens));

    crate::protocol::wire!(openai::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens,
        completion_tokens_details: thoughts.map(|reasoning_tokens| {
            crate::protocol::wire!(openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(reasoning_tokens),
                rejected_prediction_tokens: None,
                extra: Default::default(),
            })
        }),
        prompt_tokens_details: usage.cached_content_token_count.map(|tokens| {
            crate::protocol::wire!(openai::PromptTokensDetails {
                audio_tokens: None,
                cache_write_tokens: None,
                cached_tokens: Some(i32_to_u32(tokens)),
                extra: Default::default(),
            })
        }),
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn completion_usage_to_claude_box(
    usage: Option<openai::CompletionUsage>,
) -> Option<Box<claude::Usage>> {
    usage.map(|usage| Box::new(super::completion_usage_to_claude(Some(usage))))
}

pub(in crate::transform::generate_content) fn claude_usage_to_completion_option(
    usage: Option<Box<claude::Usage>>,
) -> Option<openai::CompletionUsage> {
    usage.map(|usage| super::claude_usage_to_completion(*usage))
}

pub(in crate::transform::generate_content) fn gemini_index_to_chat_index(
    index: Option<i32>,
    fallback: usize,
) -> u32 {
    index
        .map(i32_to_u32)
        .unwrap_or_else(|| u32::try_from(fallback).unwrap_or(u32::MAX))
}

pub(in crate::transform::generate_content) fn chat_index_to_gemini_index(index: u32) -> i32 {
    u32_to_i32(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_usage_roundtrip_keeps_thinking_as_output_subset() {
        let completion =
            gemini_usage_to_completion(crate::protocol::wire!(gemini::UsageMetadata {
                prompt_token_count: Some(100),
                candidates_token_count: Some(20),
                thoughts_token_count: Some(5),
                total_token_count: Some(125),
                ..Default::default()
            }));
        assert_eq!(completion.completion_tokens, 25);
        let roundtrip = completion_usage_to_gemini(Some(completion)).unwrap();
        assert_eq!(roundtrip.candidates_token_count, Some(20));
        assert_eq!(roundtrip.thoughts_token_count, Some(5));
        assert_eq!(roundtrip.total_token_count, Some(125));
    }
}
