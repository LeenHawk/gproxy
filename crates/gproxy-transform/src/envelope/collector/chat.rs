use std::collections::BTreeMap;

use gproxy_protocol::openai;

use super::SseFrame;
use crate::TransformError;

#[derive(Default)]
pub(super) struct ChatCollector {
    id: Option<String>,
    model: Option<openai::OpenAiModelId>,
    created: Option<u64>,
    choices: BTreeMap<u32, Choice>,
    usage: Option<openai::CompletionUsage>,
    service_tier: Option<openai::ServiceTier>,
    system_fingerprint: Option<String>,
    pub(super) complete: bool,
}

#[derive(Default)]
struct Choice {
    text: String,
    reasoning: String,
    refusal: String,
    function_name: Option<String>,
    function_arguments: String,
    tools: BTreeMap<u32, Tool>,
    finish_reason: Option<openai::ChatFinishReason>,
    logprobs: Option<openai::ChatChoiceLogprobs>,
}

#[derive(Default)]
struct Tool {
    id: Option<String>,
    custom: bool,
    name: Option<String>,
    data: String,
}

impl ChatCollector {
    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<(), TransformError> {
        if frame.data == "[DONE]" {
            self.complete = true;
            return Ok(());
        }
        let chunk: openai::ChatCompletionChunk = serde_json::from_str(&frame.data)?;
        if !chunk.id.is_empty() {
            self.id = Some(chunk.id);
        }
        self.model = Some(chunk.model);
        self.created = chunk.created.or(self.created);
        self.service_tier = chunk.service_tier.or(self.service_tier.take());
        self.system_fingerprint = chunk.system_fingerprint.or(self.system_fingerprint.take());
        self.usage = chunk.usage.or(self.usage.take());
        for choice in chunk.choices {
            self.choices.entry(choice.index).or_default().push(choice);
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<openai::ChatCompletionResponse, TransformError> {
        if !self.is_complete() {
            return Err(TransformError::IncompleteStream);
        }
        Ok(crate::wire!(openai::ChatCompletionResponse {
            id: self.id.unwrap_or_default(),
            choices: self
                .choices
                .into_iter()
                .map(|(index, choice)| choice.finish(index))
                .collect::<Result<_, _>>()?,
            created: self.created.or(Some(0)),
            model: self
                .model
                .unwrap_or_else(|| openai::OpenAiModelId::from("unknown")),
            object: openai::ChatCompletionObjectType::ChatCompletion,
            moderation: None,
            service_tier: self.service_tier,
            system_fingerprint: self.system_fingerprint,
            usage: self.usage,
            rest: Default::default(),
        }))
    }

    pub(super) fn is_complete(&self) -> bool {
        self.complete
            && !self.choices.is_empty()
            && self
                .choices
                .values()
                .all(|choice| choice.finish_reason.is_some())
    }
}

impl Choice {
    fn push(&mut self, choice: openai::ChatChunkChoice) {
        self.finish_reason = choice.finish_reason.or(self.finish_reason.take());
        self.logprobs = choice.logprobs.or(self.logprobs.take());
        let delta = choice.delta;
        append(&mut self.text, delta.content);
        append(&mut self.reasoning, delta.reasoning_content);
        append(&mut self.refusal, delta.refusal);
        if let Some(function) = delta.function_call {
            self.function_name = function.name.or(self.function_name.take());
            append(&mut self.function_arguments, function.arguments);
        }
        for call in delta.tool_calls.into_iter().flatten() {
            self.tools.entry(call.index).or_default().push(call);
        }
    }

    fn finish(self, index: u32) -> Result<openai::ChatCompletionChoice, TransformError> {
        let finish_reason = self.finish_reason.ok_or(TransformError::IncompleteStream)?;
        let function_call = self.function_name.map(|name| {
            crate::wire!(openai::FunctionCall {
                arguments: self.function_arguments,
                name,
                rest: Default::default(),
            })
        });
        let tools = self
            .tools
            .into_iter()
            .map(|(index, tool)| tool.finish(index))
            .collect::<Vec<_>>();
        let has_other = !self.reasoning.is_empty()
            || !self.refusal.is_empty()
            || function_call.is_some()
            || !tools.is_empty();
        Ok(crate::wire!(openai::ChatCompletionChoice {
            finish_reason,
            index,
            logprobs: self.logprobs,
            message: openai::ChatMessage {
                role: openai::ChatCompletionMessageRole::Assistant,
                content: if !self.text.is_empty() {
                    Some(self.text)
                } else if has_other {
                    None
                } else {
                    Some(String::new())
                },
                refusal: (!self.refusal.is_empty()).then_some(self.refusal),
                annotations: None,
                audio: None,
                function_call,
                reasoning_content: (!self.reasoning.is_empty()).then_some(self.reasoning),
                tool_calls: (!tools.is_empty()).then_some(tools),
                rest: Default::default(),
            },
            rest: Default::default(),
        }))
    }
}

impl Tool {
    fn push(&mut self, call: openai::ChatToolCallDelta) {
        self.id = call.id.or(self.id.take());
        self.custom |= matches!(call.type_, Some(openai::ChatToolCallType::Custom));
        if let Some(function) = call.function {
            self.custom = false;
            self.name = function.name.or(self.name.take());
            append(&mut self.data, function.arguments);
        }
        if let Some(custom) = call.custom {
            self.custom = true;
            self.name = custom.name.or(self.name.take());
            append(&mut self.data, custom.input);
        }
    }

    fn finish(self, index: u32) -> openai::ChatToolCall {
        let id = self.id.unwrap_or_else(|| format!("call_{index}"));
        if self.custom {
            openai::ChatToolCall::Custom(crate::wire!(openai::ChatCustomToolCall {
                id,
                type_: openai::CustomToolChoiceType::Custom,
                custom: openai::CustomToolCall {
                    input: self.data,
                    name: self.name.unwrap_or_default(),
                    rest: Default::default(),
                },
                rest: Default::default(),
            }))
        } else {
            openai::ChatToolCall::Function(crate::wire!(openai::ChatFunctionToolCall {
                id,
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::FunctionCall {
                    arguments: self.data,
                    name: self.name.unwrap_or_default(),
                    rest: Default::default(),
                },
                rest: Default::default(),
            }))
        }
    }
}

fn append(target: &mut String, value: Option<String>) {
    if let Some(value) = value {
        target.push_str(&value);
    }
}
