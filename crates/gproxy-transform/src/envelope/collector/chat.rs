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
    rest: openai::Rest,
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
    rest: openai::Rest,
    message_rest: openai::Rest,
}

#[derive(Default)]
struct Tool {
    id: Option<String>,
    custom: bool,
    name: Option<String>,
    data: String,
    rest: openai::Rest,
    payload_rest: openai::Rest,
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
        self.rest.extend(chunk.rest);
        self.usage = chunk.usage.or(self.usage.take());
        for choice in chunk.choices {
            self.choices.entry(choice.index).or_default().push(choice);
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<openai::ChatCompletionResponse, TransformError> {
        if !self.complete {
            return Err(TransformError::IncompleteStream);
        }
        if self.choices.is_empty() {
            self.choices.insert(0, Choice::default());
        }
        Ok(openai::ChatCompletionResponse {
            id: self.id.unwrap_or_default(),
            choices: self
                .choices
                .into_iter()
                .map(|(index, choice)| choice.finish(index))
                .collect(),
            created: self.created.or(Some(0)),
            model: self
                .model
                .unwrap_or_else(|| openai::OpenAiModelId::from("unknown")),
            object: openai::ChatCompletionObjectType::ChatCompletion,
            moderation: None,
            service_tier: self.service_tier,
            system_fingerprint: self.system_fingerprint,
            usage: self.usage,
            rest: self.rest,
        })
    }
}

impl Choice {
    fn push(&mut self, choice: openai::ChatChunkChoice) {
        self.finish_reason = choice.finish_reason.or(self.finish_reason.take());
        self.logprobs = choice.logprobs.or(self.logprobs.take());
        self.rest.extend(choice.rest);
        let delta = choice.delta;
        append(&mut self.text, delta.content);
        append(&mut self.reasoning, delta.reasoning_content);
        append(&mut self.refusal, delta.refusal);
        if let Some(function) = delta.function_call {
            self.function_name = function.name.or(self.function_name.take());
            append(&mut self.function_arguments, function.arguments);
            self.message_rest.extend(function.rest);
        }
        for call in delta.tool_calls.into_iter().flatten() {
            self.tools.entry(call.index).or_default().push(call);
        }
        if let Some(obfuscation) = delta.obfuscation {
            self.message_rest
                .insert("obfuscation".into(), obfuscation.into());
        }
        self.message_rest.extend(delta.rest);
    }

    fn finish(self, index: u32) -> openai::ChatCompletionChoice {
        let function_call = self.function_name.map(|name| openai::FunctionCall {
            arguments: self.function_arguments,
            name,
            rest: Default::default(),
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
        openai::ChatCompletionChoice {
            finish_reason: self.finish_reason.unwrap_or(openai::ChatFinishReason::Stop),
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
                rest: self.message_rest,
            },
            rest: self.rest,
        }
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
            self.payload_rest.extend(function.rest);
        }
        if let Some(custom) = call.custom {
            self.custom = true;
            self.name = custom.name.or(self.name.take());
            append(&mut self.data, custom.input);
            self.payload_rest.extend(custom.rest);
        }
        self.rest.extend(call.rest);
    }

    fn finish(self, index: u32) -> openai::ChatToolCall {
        let id = self.id.unwrap_or_else(|| format!("call_{index}"));
        if self.custom {
            openai::ChatToolCall::Custom(openai::ChatCustomToolCall {
                id,
                type_: openai::CustomToolChoiceType::Custom,
                custom: openai::CustomToolCall {
                    input: self.data,
                    name: self.name.unwrap_or_default(),
                    rest: self.payload_rest,
                },
                rest: self.rest,
            })
        } else {
            openai::ChatToolCall::Function(openai::ChatFunctionToolCall {
                id,
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::FunctionCall {
                    arguments: self.data,
                    name: self.name.unwrap_or_default(),
                    rest: self.payload_rest,
                },
                rest: self.rest,
            })
        }
    }
}

fn append(target: &mut String, value: Option<String>) {
    if let Some(value) = value {
        target.push_str(&value);
    }
}
