use crate::protocol::{claude, gemini, openai};

use super::scalar::{i32_to_u32, u32_to_i32};

pub(in crate::transform::generate_content) fn default_openai_model() -> openai::OpenAiModelId {
    super::DEFAULT_OPENAI_MODEL.to_owned().into()
}

pub(in crate::transform::generate_content) fn empty_chat_delta() -> openai::ChatDelta {
    openai::ChatDelta {
        role: None,
        content: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: None,
        function_call: None,
        obfuscation: None,
        extra: Default::default(),
    }
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
    openai::ChatCompletionChunk {
        id,
        choices: vec![openai::ChatChunkChoice {
            index,
            delta,
            finish_reason,
            logprobs: None,
            extra: Default::default(),
        }],
        created,
        model,
        object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
        service_tier: None,
        system_fingerprint: None,
        usage,
        extra: Default::default(),
    }
}

pub(in crate::transform::generate_content) fn empty_chat_chunk(
    id: String,
    model: openai::OpenAiModelId,
    created: u64,
    usage: Option<openai::CompletionUsage>,
) -> openai::ChatCompletionChunk {
    openai::ChatCompletionChunk {
        id,
        choices: Vec::new(),
        created,
        model,
        object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
        service_tier: None,
        system_fingerprint: None,
        usage,
        extra: Default::default(),
    }
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
    openai::ChatToolCallDelta {
        index,
        id,
        type_: Some(openai::ChatToolCallType::Function),
        function: Some(openai::FunctionCallDelta {
            arguments,
            name,
            extra: Default::default(),
        }),
        custom: None,
        extra: Default::default(),
    }
}

pub(in crate::transform::generate_content) fn chat_custom_tool_delta(
    index: u32,
    id: Option<String>,
    name: Option<String>,
    input: Option<String>,
) -> openai::ChatToolCallDelta {
    openai::ChatToolCallDelta {
        index,
        id,
        type_: Some(openai::ChatToolCallType::Custom),
        function: None,
        custom: Some(openai::CustomToolCallDelta {
            input,
            name,
            extra: Default::default(),
        }),
        extra: Default::default(),
    }
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
    }
}

pub(in crate::transform::generate_content) fn completion_usage_to_gemini(
    usage: Option<openai::CompletionUsage>,
) -> Option<gemini::UsageMetadata> {
    let usage = usage?;
    Some(gemini::UsageMetadata {
        prompt_token_count: Some(u32_to_i32(usage.prompt_tokens)),
        cached_content_token_count: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .map(u32_to_i32),
        candidates_token_count: Some(u32_to_i32(usage.completion_tokens)),
        thoughts_token_count: usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens)
            .map(u32_to_i32),
        total_token_count: Some(u32_to_i32(usage.total_tokens)),
        tool_use_prompt_token_count: None,
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn response_usage_to_completion(
    usage: Option<openai::ResponseUsage>,
) -> Option<openai::CompletionUsage> {
    let usage = usage?;
    let cached_tokens = usage
        .input_tokens_details
        .map(|details| details.cached_tokens);
    let reasoning_tokens = usage.output_tokens_details.reasoning_tokens;

    Some(openai::CompletionUsage {
        completion_tokens: usage.output_tokens,
        prompt_tokens: usage.input_tokens,
        total_tokens: usage.total_tokens,
        completion_tokens_details: (reasoning_tokens > 0).then_some(
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(reasoning_tokens),
                rejected_prediction_tokens: None,
                extra: Default::default(),
            },
        ),
        prompt_tokens_details: cached_tokens.map(|cached_tokens| openai::PromptTokensDetails {
            audio_tokens: None,
            cached_tokens: Some(cached_tokens),
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn completion_usage_to_response(
    usage: Option<openai::CompletionUsage>,
) -> Option<openai::ResponseUsage> {
    let usage = usage?;
    let cached_tokens = usage
        .prompt_tokens_details
        .and_then(|details| details.cached_tokens);
    let reasoning_tokens = usage
        .completion_tokens_details
        .and_then(|details| details.reasoning_tokens)
        .unwrap_or_default();

    Some(openai::ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: cached_tokens.map(|cached_tokens| {
            openai::ResponseInputTokensDetails {
                cached_tokens,
                extra: Default::default(),
            }
        }),
        output_tokens_details: openai::ResponseOutputTokensDetails {
            reasoning_tokens,
            extra: Default::default(),
        },
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn gemini_usage_to_completion(
    usage: gemini::UsageMetadata,
) -> openai::CompletionUsage {
    let prompt_tokens = usage.prompt_token_count.map(i32_to_u32).unwrap_or_default();
    let completion_tokens = usage
        .candidates_token_count
        .map(i32_to_u32)
        .unwrap_or_default();
    let total_tokens = usage
        .total_token_count
        .map(i32_to_u32)
        .unwrap_or_else(|| prompt_tokens.saturating_add(completion_tokens));

    openai::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens,
        completion_tokens_details: usage.thoughts_token_count.map(|tokens| {
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(i32_to_u32(tokens)),
                rejected_prediction_tokens: None,
                extra: Default::default(),
            }
        }),
        prompt_tokens_details: usage.cached_content_token_count.map(|tokens| {
            openai::PromptTokensDetails {
                audio_tokens: None,
                cached_tokens: Some(i32_to_u32(tokens)),
                extra: Default::default(),
            }
        }),
        extra: Default::default(),
    }
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
