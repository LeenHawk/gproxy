use std::collections::BTreeMap;

use gproxy_protocol::openai;

use super::SseFrame;
use crate::TransformError;

#[derive(Default)]
pub(super) struct ChatCollector {
    id: Option<String>,
    model: Option<openai::OpenAiModelId>,
    created: Option<u64>,
    text: String,
    reasoning: String,
    tools: BTreeMap<u32, ChatTool>,
    finish_reason: Option<openai::ChatFinishReason>,
    usage: Option<openai::CompletionUsage>,
    rest: openai::Rest,
    pub(super) complete: bool,
}

#[derive(Default)]
struct ChatTool {
    id: String,
    name: String,
    arguments: String,
    rest: openai::Rest,
}

impl ChatCollector {
    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<(), TransformError> {
        if frame.data == "[DONE]" {
            self.complete = true;
            return Ok(());
        }
        let chunk: openai::ChatCompletionChunk = serde_json::from_str(&frame.data)?;
        self.id.get_or_insert(chunk.id);
        self.model.get_or_insert(chunk.model);
        self.created = chunk.created;
        self.rest.extend(chunk.rest);
        if chunk.usage.is_some() {
            self.usage = chunk.usage;
        }
        for choice in chunk.choices {
            if choice.index != 0 {
                return Err(TransformError::unsupported(
                    "Chat stream",
                    "multiple choices",
                ));
            }
            self.text
                .push_str(choice.delta.content.as_deref().unwrap_or(""));
            self.reasoning
                .push_str(choice.delta.reasoning_content.as_deref().unwrap_or(""));
            for call in choice.delta.tool_calls.into_iter().flatten() {
                let tool = self.tools.entry(call.index).or_default();
                if let Some(id) = call.id {
                    tool.id = id;
                }
                if let Some(function) = call.function {
                    tool.name.push_str(function.name.as_deref().unwrap_or(""));
                    tool.arguments
                        .push_str(function.arguments.as_deref().unwrap_or(""));
                    tool.rest.extend(function.rest);
                }
                tool.rest.extend(call.rest);
            }
            if choice.finish_reason.is_some() {
                self.finish_reason = choice.finish_reason;
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<openai::ChatCompletionResponse, TransformError> {
        if !self.complete {
            return Err(TransformError::IncompleteStream);
        }
        let calls = self
            .tools
            .into_values()
            .map(|tool| {
                openai::ChatToolCall::Function(openai::ChatFunctionToolCall {
                    id: tool.id,
                    type_: openai::FunctionToolChoiceType::Function,
                    function: openai::FunctionCall {
                        arguments: tool.arguments,
                        name: tool.name,
                        rest: tool.rest,
                    },
                    rest: Default::default(),
                })
            })
            .collect::<Vec<_>>();
        Ok(openai::ChatCompletionResponse {
            id: self
                .id
                .ok_or_else(|| TransformError::shape("Chat stream", "id is missing"))?,
            choices: vec![openai::ChatCompletionChoice {
                finish_reason: self.finish_reason.ok_or(TransformError::IncompleteStream)?,
                index: 0,
                logprobs: None,
                message: openai::ChatMessage {
                    role: openai::ChatCompletionMessageRole::Assistant,
                    content: (!self.text.is_empty()).then_some(self.text),
                    refusal: None,
                    annotations: None,
                    audio: None,
                    function_call: None,
                    reasoning_content: (!self.reasoning.is_empty()).then_some(self.reasoning),
                    tool_calls: (!calls.is_empty()).then_some(calls),
                    rest: Default::default(),
                },
                rest: Default::default(),
            }],
            created: self.created,
            model: self
                .model
                .ok_or_else(|| TransformError::shape("Chat stream", "model is missing"))?,
            object: openai::ChatCompletionObjectType::ChatCompletion,
            moderation: None,
            service_tier: None,
            system_fingerprint: None,
            usage: self.usage,
            rest: self.rest,
        })
    }
}
