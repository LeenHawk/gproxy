use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: openai::ChatCompletionChunk,
    _: &TransformContext,
) -> Result<gemini::StreamGenerateContentChunk, TransformError> {
    Ok(chat_chunk_to_gemini(input))
}

fn chat_chunk_to_gemini(input: openai::ChatCompletionChunk) -> gemini::StreamGenerateContentChunk {
    crate::protocol::wire!(gemini::GenerateContentResponse {
        candidates: input
            .choices
            .into_iter()
            .map(|choice| crate::protocol::wire!(gemini::Candidate {
                content: chat_delta_to_gemini_content(choice.delta),
                finish_reason: choice
                    .finish_reason
                    .map(common::chat_finish_reason_to_gemini),
                safety_ratings: Vec::new(),
                citation_metadata: None,
                token_count: None,
                grounding_metadata: None,
                avg_logprobs: None,
                logprobs_result: None,
                url_context_metadata: None,
                index: Some(common::chat_index_to_gemini_index(choice.index)),
                finish_message: None,
                extra: Default::default(),
            }))
            .collect(),
        prompt_feedback: None,
        usage_metadata: common::completion_usage_to_gemini(input.usage),
        model_version: Some(common::openai_model_string(input.model)),
        response_id: Some(input.id),
        model_status: None,
        extra: Default::default(),
    })
}

fn chat_delta_to_gemini_content(delta: openai::ChatDelta) -> Option<gemini::Content> {
    let mut parts = Vec::new();

    if let Some(content) = delta.content.filter(|value| !value.is_empty()) {
        parts.push(gemini_text_part(content, false));
    }
    if let Some(reasoning) = delta.reasoning_content.filter(|value| !value.is_empty()) {
        parts.push(gemini_text_part(reasoning, true));
    }
    if let Some(refusal) = delta.refusal.filter(|value| !value.is_empty()) {
        parts.push(gemini_text_part(refusal, false));
    }
    if let Some(tool_calls) = delta.tool_calls {
        parts.extend(
            tool_calls
                .into_iter()
                .filter_map(chat_tool_delta_to_gemini_part),
        );
    }
    if let Some(function_call) = delta.function_call
        && let Some(name) = function_call.name.filter(|value| !value.is_empty())
    {
        parts.push(gemini_function_call_part(
            None,
            name,
            function_call.arguments.and_then(arguments_to_json_map),
            None,
        ));
    }

    (!parts.is_empty()).then_some(crate::protocol::wire!(gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
        extra: Default::default(),
    }))
}

fn chat_tool_delta_to_gemini_part(mut call: openai::ChatToolCallDelta) -> Option<gemini::Part> {
    let thought_signature = take_thought_signature(&mut call.extra);
    if let Some(function) = call.function {
        return function.name.filter(|value| !value.is_empty()).map(|name| {
            gemini_function_call_part(
                call.id,
                name,
                function.arguments.and_then(arguments_to_json_map),
                thought_signature.clone(),
            )
        });
    }

    if let Some(custom) = call.custom {
        return custom.name.filter(|value| !value.is_empty()).map(|name| {
            gemini_function_call_part(
                call.id,
                name,
                custom.input.and_then(arguments_to_json_map),
                thought_signature,
            )
        });
    }

    None
}

fn gemini_function_call_part(
    id: Option<String>,
    name: String,
    args: Option<gemini::JsonMap>,
    thought_signature: Option<String>,
) -> gemini::Part {
    crate::protocol::wire!(gemini::Part {
        thought_signature,
        data: Some(crate::protocol::wire!(gemini::PartData::FunctionCall {
            function_call: crate::protocol::wire!(gemini::FunctionCall {
                id,
                name,
                args,
                extra: Default::default(),
            }),
        })),
        ..Default::default()
    })
}

fn take_thought_signature(extra: &mut openai::Extra) -> Option<String> {
    extra
        .remove("thought_signature")
        .or_else(|| extra.remove("thoughtSignature"))
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn gemini_text_part(text: String, thought: bool) -> gemini::Part {
    crate::protocol::wire!(gemini::Part {
        thought: thought.then_some(true),
        data: Some(gemini::PartData::Text { text }),
        ..Default::default()
    })
}

fn arguments_to_json_map(value: String) -> Option<gemini::JsonMap> {
    serde_json::from_str(&value).ok()
}
