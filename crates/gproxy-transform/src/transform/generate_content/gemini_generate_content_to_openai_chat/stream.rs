use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: gemini::StreamGenerateContentChunk,
    _: &TransformContext,
) -> Result<openai::ChatCompletionChunk, TransformError> {
    Ok(gemini_chunk_to_chat(input))
}

fn gemini_chunk_to_chat(input: gemini::StreamGenerateContentChunk) -> openai::ChatCompletionChunk {
    let id = input.response_id.unwrap_or_default();
    let model = input
        .model_version
        .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned())
        .into();
    let usage_metadata = input.usage_metadata;
    let service_tier = usage_metadata
        .as_ref()
        .and_then(|usage| common::gemini_service_tier_to_openai(usage.service_tier.clone()));
    let usage = usage_metadata.map(common::gemini_usage_to_completion);
    let blocked = input
        .prompt_feedback
        .as_ref()
        .and_then(|feedback| feedback.block_reason.as_ref())
        .is_some();

    if input.candidates.is_empty() && blocked {
        return common::chat_finish_chunk(
            id,
            model,
            0,
            openai::ChatFinishReason::ContentFilter,
            usage,
        );
    }

    let choices = input
        .candidates
        .into_iter()
        .enumerate()
        .map(|(fallback_index, candidate)| openai::ChatChunkChoice {
            index: common::gemini_index_to_chat_index(candidate.index, fallback_index),
            delta: gemini_content_to_chat_delta(candidate.content),
            finish_reason: candidate
                .finish_reason
                .map(common::gemini_finish_reason_to_chat),
            logprobs: None,
            extra: Default::default(),
        })
        .collect();

    openai::ChatCompletionChunk {
        id,
        choices,
        created: 0,
        model,
        object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
        service_tier,
        system_fingerprint: None,
        usage,
        extra: Default::default(),
    }
}

fn gemini_content_to_chat_delta(content: Option<gemini::Content>) -> openai::ChatDelta {
    let mut delta = common::empty_chat_delta();
    let Some(content) = content else {
        return delta;
    };

    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut tool_calls = Vec::new();

    for (index, part) in content.parts.into_iter().enumerate() {
        match part.data {
            Some(gemini::PartData::Text { text: value }) => {
                if part.thought.unwrap_or(false) {
                    reasoning.push(value);
                } else {
                    text.push(value);
                }
            }
            Some(gemini::PartData::FunctionCall { function_call }) => {
                tool_calls.push(common::chat_function_tool_delta(
                    u32::try_from(index).unwrap_or(u32::MAX),
                    function_call.id,
                    Some(function_call.name),
                    function_call.args.map(json_map_to_string),
                ));
            }
            _ => {}
        }
    }

    if !text.is_empty() {
        delta.content = Some(text.join(""));
    }
    if !reasoning.is_empty() {
        delta.reasoning_content = Some(reasoning.join(""));
    }
    if !tool_calls.is_empty() {
        delta.tool_calls = Some(tool_calls);
    }

    delta
}

fn json_map_to_string(value: gemini::JsonMap) -> String {
    serde_json::to_string(&value).unwrap_or_default()
}
