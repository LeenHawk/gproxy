use std::collections::BTreeMap;

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: gemini::StreamGenerateContentChunk,
    ctx: &TransformContext,
) -> Result<openai::ChatCompletionChunk, TransformError> {
    StreamTransform::default().push(input, ctx)
}

#[derive(Default)]
pub struct StreamTransform {
    choices: BTreeMap<u32, ChoiceState>,
}

#[derive(Default)]
struct ChoiceState {
    next_tool_call_index: u32,
    had_tool_calls: bool,
}

impl StreamTransform {
    pub fn push(
        &mut self,
        input: gemini::StreamGenerateContentChunk,
        _: &TransformContext,
    ) -> Result<openai::ChatCompletionChunk, TransformError> {
        Ok(self.gemini_chunk_to_chat(input))
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        Ok(Vec::new())
    }

    fn gemini_chunk_to_chat(
        &mut self,
        input: gemini::StreamGenerateContentChunk,
    ) -> openai::ChatCompletionChunk {
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
            .map(|(fallback_index, candidate)| {
                let index = common::gemini_index_to_chat_index(candidate.index, fallback_index);
                let state = self.choices.entry(index).or_default();
                let delta = gemini_content_to_chat_delta(
                    candidate.content,
                    &mut state.next_tool_call_index,
                );
                state.had_tool_calls |= delta
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| !calls.is_empty());
                let finish_reason = candidate.finish_reason.map(|reason| {
                    let finish_reason = common::gemini_finish_reason_to_chat(reason);
                    if state.had_tool_calls && finish_reason == openai::ChatFinishReason::Stop {
                        openai::ChatFinishReason::ToolCalls
                    } else {
                        finish_reason
                    }
                });
                crate::protocol::wire!(openai::ChatChunkChoice {
                    index,
                    delta,
                    finish_reason,
                    logprobs: None,
                    extra: Default::default(),
                })
            })
            .collect();

        crate::protocol::wire!(openai::ChatCompletionChunk {
            id,
            choices,
            created: 0,
            model,
            object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
            service_tier,
            system_fingerprint: None,
            usage,
            extra: Default::default(),
        })
    }
}

fn gemini_content_to_chat_delta(
    content: Option<gemini::Content>,
    next_tool_call_index: &mut u32,
) -> openai::ChatDelta {
    let mut delta = common::empty_chat_delta();
    let Some(content) = content else {
        return delta;
    };

    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut tool_calls = Vec::new();

    for part in content.parts {
        let thought_signature = part.thought_signature;
        match part.data {
            Some(gemini::PartData::Text { text: value }) => {
                if part.thought.unwrap_or(false) {
                    reasoning.push(value);
                } else {
                    text.push(value);
                }
            }
            Some(gemini::PartData::FunctionCall { function_call }) => {
                let mut tool_call = common::chat_function_tool_delta(
                    *next_tool_call_index,
                    function_call.id,
                    Some(function_call.name),
                    function_call.args.map(json_map_to_string),
                );
                tool_call.extra = thought_signature_extra(thought_signature);
                tool_calls.push(tool_call);
                *next_tool_call_index = next_tool_call_index.saturating_add(1);
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

fn thought_signature_extra(signature: Option<String>) -> openai::Extra {
    signature
        .map(|signature| {
            [(
                "thought_signature".to_owned(),
                serde_json::Value::String(signature),
            )]
            .into_iter()
            .collect()
        })
        .unwrap_or_default()
}

fn json_map_to_string(value: gemini::JsonMap) -> String {
    serde_json::to_string(&value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiChatCompletions,
            ),
        )
    }

    fn chunk(
        parts: serde_json::Value,
        finish_reason: Option<&str>,
    ) -> gemini::GenerateContentResponse {
        let mut candidate = json!({
            "index": 0,
            "content": {"role": "model", "parts": parts}
        });
        if let Some(reason) = finish_reason {
            candidate["finishReason"] = json!(reason);
        }
        serde_json::from_value(json!({
            "responseId": "r1",
            "modelVersion": "gemini-test",
            "candidates": [candidate]
        }))
        .unwrap()
    }

    #[test]
    fn parallel_tool_calls_across_chunks_get_distinct_indices_and_terminal_reason() {
        let mut transform = StreamTransform::default();
        let first = transform
            .push(
                chunk(
                    json!([{
                        "functionCall": {"id": "call_1", "name": "weather", "args": {"city": "北京"}},
                        "thoughtSignature": "ciphertext"
                    }]),
                    None,
                ),
                &ctx(),
            )
            .unwrap();
        let second = transform
            .push(
                chunk(
                    json!([{
                        "functionCall": {"id": "call_2", "name": "weather", "args": {"city": "上海"}}
                    }]),
                    None,
                ),
                &ctx(),
            )
            .unwrap();
        let terminal = transform
            .push(chunk(json!([]), Some("STOP")), &ctx())
            .unwrap();

        let first_call = &first.choices[0].delta.tool_calls.as_ref().unwrap()[0];
        let second_call = &second.choices[0].delta.tool_calls.as_ref().unwrap()[0];
        assert_eq!(first_call.index, 0);
        assert_eq!(second_call.index, 1);
        assert_eq!(first_call.extra["thought_signature"], "ciphertext");
        assert_eq!(
            terminal.choices[0].finish_reason,
            Some(openai::ChatFinishReason::ToolCalls)
        );
    }

    #[test]
    fn text_parts_do_not_shift_tool_call_indices() {
        let mut transform = StreamTransform::default();
        let output = transform
            .push(
                chunk(
                    json!([
                        {"text": "先查两个城市"},
                        {"functionCall": {"id": "call_1", "name": "weather", "args": {}}},
                        {"functionCall": {"id": "call_2", "name": "weather", "args": {}}}
                    ]),
                    None,
                ),
                &ctx(),
            )
            .unwrap();

        let calls = output.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(
            calls.iter().map(|call| call.index).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }
}
