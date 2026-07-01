use std::collections::BTreeMap;

use crate::protocol::openai;

use super::super::common;

pub fn response(
    chunks: impl IntoIterator<Item = openai::ChatCompletionChunk>,
) -> openai::ChatCompletionResponse {
    let mut collector = ChatCollector::default();
    for chunk in chunks {
        collector.push(chunk);
    }
    collector.finish()
}

#[derive(Default)]
struct ChatCollector {
    id: Option<String>,
    created: Option<u64>,
    model: Option<openai::OpenAiModelId>,
    service_tier: Option<openai::ServiceTier>,
    system_fingerprint: Option<String>,
    usage: Option<openai::CompletionUsage>,
    choices: BTreeMap<u32, ChoiceState>,
}

impl ChatCollector {
    fn push(&mut self, chunk: openai::ChatCompletionChunk) {
        if !chunk.id.is_empty() {
            self.id = Some(chunk.id);
        }
        if chunk.created > 0 {
            self.created = Some(chunk.created);
        }
        self.model = Some(chunk.model);
        self.service_tier = chunk.service_tier.or(self.service_tier.take());
        self.system_fingerprint = chunk.system_fingerprint.or(self.system_fingerprint.take());
        self.usage = chunk.usage.or(self.usage.take());

        for choice in chunk.choices {
            self.choices
                .entry(choice.index)
                .or_insert_with(|| ChoiceState::new(choice.index))
                .push(choice);
        }
    }

    fn finish(mut self) -> openai::ChatCompletionResponse {
        if self.choices.is_empty() {
            self.choices.insert(0, ChoiceState::new(0));
        }

        openai::ChatCompletionResponse {
            id: self.id.unwrap_or_default(),
            choices: self
                .choices
                .into_values()
                .map(ChoiceState::finish)
                .collect(),
            created: self.created.unwrap_or_default(),
            model: self.model.unwrap_or_else(common::default_openai_model),
            object: openai::ChatCompletionObjectType::ChatCompletion,
            moderation: None,
            service_tier: self.service_tier,
            system_fingerprint: self.system_fingerprint,
            usage: self.usage,
            extra: Default::default(),
        }
    }
}

struct ChoiceState {
    index: u32,
    content: String,
    reasoning_content: String,
    refusal: String,
    function_call: LegacyFunctionState,
    tool_calls: BTreeMap<u32, ToolCallState>,
    finish_reason: Option<openai::ChatFinishReason>,
    logprobs: Option<openai::ChatChoiceLogprobs>,
}

impl ChoiceState {
    fn new(index: u32) -> Self {
        Self {
            index,
            content: String::new(),
            reasoning_content: String::new(),
            refusal: String::new(),
            function_call: LegacyFunctionState::default(),
            tool_calls: BTreeMap::new(),
            finish_reason: None,
            logprobs: None,
        }
    }

    fn push(&mut self, choice: openai::ChatChunkChoice) {
        self.finish_reason = choice.finish_reason.or(self.finish_reason.take());
        self.logprobs = choice.logprobs.or(self.logprobs.take());

        let delta = choice.delta;
        append_opt(&mut self.content, delta.content);
        append_opt(&mut self.reasoning_content, delta.reasoning_content);
        append_opt(&mut self.refusal, delta.refusal);

        if let Some(function_call) = delta.function_call {
            self.function_call.push(function_call);
        }

        if let Some(tool_calls) = delta.tool_calls {
            for call in tool_calls {
                self.tool_calls
                    .entry(call.index)
                    .or_insert_with(|| ToolCallState::new(call.index))
                    .push(call);
            }
        }
    }

    fn finish(self) -> openai::ChatCompletionChoice {
        let tool_calls = self
            .tool_calls
            .into_values()
            .filter_map(ToolCallState::finish)
            .collect::<Vec<_>>();
        let function_call = self.function_call.finish();
        let has_message_data = !self.content.is_empty()
            || !self.reasoning_content.is_empty()
            || !self.refusal.is_empty()
            || function_call.is_some()
            || !tool_calls.is_empty();

        openai::ChatCompletionChoice {
            finish_reason: self.finish_reason.unwrap_or(openai::ChatFinishReason::Stop),
            index: self.index,
            logprobs: self.logprobs,
            message: openai::ChatMessage {
                role: openai::ChatCompletionMessageRole::Assistant,
                content: if !self.content.is_empty() {
                    Some(self.content)
                } else if !has_message_data {
                    Some(String::new())
                } else {
                    None
                },
                refusal: (!self.refusal.is_empty()).then_some(self.refusal),
                annotations: None,
                audio: None,
                function_call,
                reasoning_content: (!self.reasoning_content.is_empty())
                    .then_some(self.reasoning_content),
                tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
                extra: Default::default(),
            },
            extra: Default::default(),
        }
    }
}

#[derive(Default)]
struct LegacyFunctionState {
    name: Option<String>,
    arguments: String,
}

impl LegacyFunctionState {
    fn push(&mut self, delta: openai::FunctionCallDelta) {
        self.name = delta.name.or(self.name.take());
        append_opt(&mut self.arguments, delta.arguments);
    }

    fn finish(self) -> Option<openai::FunctionCall> {
        let name = self.name?;
        Some(openai::FunctionCall {
            name,
            arguments: self.arguments,
            extra: Default::default(),
        })
    }
}

struct ToolCallState {
    index: u32,
    id: Option<String>,
    kind: Option<ToolCallKind>,
    function_name: Option<String>,
    function_arguments: String,
    custom_name: Option<String>,
    custom_input: String,
}

impl ToolCallState {
    fn new(index: u32) -> Self {
        Self {
            index,
            id: None,
            kind: None,
            function_name: None,
            function_arguments: String::new(),
            custom_name: None,
            custom_input: String::new(),
        }
    }

    fn push(&mut self, delta: openai::ChatToolCallDelta) {
        self.id = delta.id.or(self.id.take());
        self.kind = delta.type_.map(ToolCallKind::from).or(self.kind.take());

        if let Some(function) = delta.function {
            self.kind = Some(ToolCallKind::Function);
            self.function_name = function.name.or(self.function_name.take());
            append_opt(&mut self.function_arguments, function.arguments);
        }

        if let Some(custom) = delta.custom {
            self.kind = Some(ToolCallKind::Custom);
            self.custom_name = custom.name.or(self.custom_name.take());
            append_opt(&mut self.custom_input, custom.input);
        }
    }

    fn finish(self) -> Option<openai::ChatToolCall> {
        let id = self.id.unwrap_or_else(|| format!("call_{}", self.index));
        match self.kind.unwrap_or(ToolCallKind::Function) {
            ToolCallKind::Function => Some(openai::ChatToolCall::Function {
                id,
                function: openai::FunctionCall {
                    name: self.function_name.unwrap_or_default(),
                    arguments: self.function_arguments,
                    extra: Default::default(),
                },
                extra: Default::default(),
            }),
            ToolCallKind::Custom => Some(openai::ChatToolCall::Custom {
                id,
                custom: openai::CustomToolCall {
                    name: self.custom_name.unwrap_or_default(),
                    input: self.custom_input,
                    extra: Default::default(),
                },
                extra: Default::default(),
            }),
        }
    }
}

#[derive(Clone, Copy)]
enum ToolCallKind {
    Function,
    Custom,
}

impl From<openai::ChatToolCallType> for ToolCallKind {
    fn from(value: openai::ChatToolCallType) -> Self {
        match value {
            openai::ChatToolCallType::Function => Self::Function,
            openai::ChatToolCallType::Custom => Self::Custom,
        }
    }
}

fn append_opt(target: &mut String, value: Option<String>) {
    if let Some(value) = value {
        target.push_str(&value);
    }
}
