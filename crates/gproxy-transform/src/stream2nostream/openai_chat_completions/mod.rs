use std::collections::BTreeMap;

use gproxy_protocol::openai::create_chat_completions::response::{
    ChatCompletionChoice, ChatCompletionObjectType, CreateChatCompletionResponse,
};
use gproxy_protocol::openai::create_chat_completions::stream::CreateChatCompletionStreamResponse;
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionChoiceLogprobs, ChatCompletionFinishReason, ChatCompletionFunctionCall,
    ChatCompletionFunctionCallDelta, ChatCompletionMessageToolCall,
    ChatCompletionMessageToolCallChunk, ChatCompletionMessageToolCallFunction,
    ChatCompletionResponseMessage, ChatCompletionResponseRole, ChatCompletionRole,
    ChatCompletionToolCallChunkType, CompletionUsage, ServiceTier,
};

#[derive(Debug, Clone)]
struct ToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Clone)]
struct ChoiceState {
    role: ChatCompletionResponseRole,
    content: String,
    refusal: String,
    tool_calls: BTreeMap<i64, ToolCallState>,
    function_call: Option<ChatCompletionFunctionCall>,
    logprobs: Option<ChatCompletionChoiceLogprobs>,
    finish_reason: Option<ChatCompletionFinishReason>,
}

#[derive(Debug, Clone)]
pub struct OpenAIChatCompletionStreamToResponseState {
    id: String,
    model: String,
    created: i64,
    usage: Option<CompletionUsage>,
    service_tier: Option<ServiceTier>,
    system_fingerprint: Option<String>,
    choices: BTreeMap<i64, ChoiceState>,
}

impl OpenAIChatCompletionStreamToResponseState {
    pub fn new() -> Self {
        Self {
            id: "chatcmpl".to_string(),
            model: "unknown".to_string(),
            created: 0,
            usage: None,
            service_tier: None,
            system_fingerprint: None,
            choices: BTreeMap::new(),
        }
    }

    pub fn push_chunk(
        &mut self,
        chunk: CreateChatCompletionStreamResponse,
    ) -> Option<CreateChatCompletionResponse> {
        self.update_from_chunk(&chunk);

        for choice in chunk.choices {
            let state = self.ensure_choice(choice.index);
            let delta = choice.delta;

            if let Some(role) = delta.role
                && matches!(role, ChatCompletionRole::Assistant) {
                    state.role = ChatCompletionResponseRole::Assistant;
                }

            if let Some(content) = delta.content {
                state.content.push_str(&content);
            }

            if let Some(refusal) = delta.refusal {
                state.refusal.push_str(&refusal);
            }

            if let Some(function_call) = delta.function_call {
                merge_function_call(state, function_call);
            }

            if let Some(tool_calls) = delta.tool_calls {
                for tool_call in tool_calls {
                    merge_tool_call(state, tool_call);
                }
            }

            if let Some(logprobs) = choice.logprobs {
                merge_logprobs(&mut state.logprobs, logprobs);
            }

            if let Some(reason) = choice.finish_reason {
                state.finish_reason = Some(reason);
            }
        }

        if self.is_finished() {
            Some(self.build_response())
        } else {
            None
        }
    }

    pub fn finalize(&self) -> CreateChatCompletionResponse {
        self.build_response()
    }

    pub fn finalize_on_eof(&self) -> CreateChatCompletionResponse {
        self.build_response_with_finish_fallback(ChatCompletionFinishReason::Length)
    }

    fn update_from_chunk(&mut self, chunk: &CreateChatCompletionStreamResponse) {
        self.id = chunk.id.clone();
        self.model = chunk.model.clone();
        self.created = chunk.created;
        if chunk.usage.is_some() {
            self.usage = chunk.usage.clone();
        }
        if chunk.service_tier.is_some() {
            self.service_tier = chunk.service_tier;
        }
        if chunk.system_fingerprint.is_some() {
            self.system_fingerprint = chunk.system_fingerprint.clone();
        }
    }

    fn ensure_choice(&mut self, index: i64) -> &mut ChoiceState {
        self.choices.entry(index).or_insert_with(|| ChoiceState {
            role: ChatCompletionResponseRole::Assistant,
            content: String::new(),
            refusal: String::new(),
            tool_calls: BTreeMap::new(),
            function_call: None,
            logprobs: None,
            finish_reason: None,
        })
    }


    fn build_response(&self) -> CreateChatCompletionResponse {
        let mut choices: Vec<ChatCompletionChoice> = self
            .choices
            .iter()
            .map(|(index, state)| ChatCompletionChoice {
                index: *index,
                message: build_message(*index, state),
                finish_reason: state.finish_reason.unwrap_or(ChatCompletionFinishReason::Stop),
                logprobs: state.logprobs.clone(),
            })
            .collect();

        choices.sort_by_key(|choice| choice.index);

        CreateChatCompletionResponse {
            id: self.id.clone(),
            object: ChatCompletionObjectType::ChatCompletion,
            created: self.created,
            model: self.model.clone(),
            choices,
            usage: self.usage.clone(),
            service_tier: self.service_tier,
            system_fingerprint: self.system_fingerprint.clone(),
        }
    }

    fn build_response_with_finish_fallback(
        &self,
        fallback: ChatCompletionFinishReason,
    ) -> CreateChatCompletionResponse {
        let mut choices: Vec<ChatCompletionChoice> = self
            .choices
            .iter()
            .map(|(index, state)| ChatCompletionChoice {
                index: *index,
                message: build_message(*index, state),
                finish_reason: state.finish_reason.unwrap_or(fallback),
                logprobs: state.logprobs.clone(),
            })
            .collect();

        choices.sort_by_key(|choice| choice.index);

        CreateChatCompletionResponse {
            id: self.id.clone(),
            object: ChatCompletionObjectType::ChatCompletion,
            created: self.created,
            model: self.model.clone(),
            choices,
            usage: self.usage.clone(),
            service_tier: self.service_tier,
            system_fingerprint: self.system_fingerprint.clone(),
        }
    }

    fn is_finished(&self) -> bool {
        if self.choices.is_empty() {
            return false;
        }
        self.choices
            .values()
            .all(|choice| choice.finish_reason.is_some())
    }
}

impl Default for OpenAIChatCompletionStreamToResponseState {
    fn default() -> Self {
        Self::new()
    }
}

fn build_message(index: i64, state: &ChoiceState) -> ChatCompletionResponseMessage {
    let content = if state.content.is_empty() {
        None
    } else {
        Some(state.content.clone())
    };
    let refusal = if state.refusal.is_empty() {
        None
    } else {
        Some(state.refusal.clone())
    };
    let tool_calls = if state.tool_calls.is_empty() {
        None
    } else {
        Some(
            state
                .tool_calls
                .iter()
                .map(|(idx, tool)| ChatCompletionMessageToolCall::Function {
                    id: tool
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("tool_call_{index}_{idx}")),
                    function: ChatCompletionMessageToolCallFunction {
                        name: tool
                            .name
                            .clone()
                            .unwrap_or_else(|| "tool".to_string()),
                        arguments: tool.arguments.clone(),
                    },
                })
                .collect(),
        )
    };

    ChatCompletionResponseMessage {
        role: state.role,
        content,
        refusal,
        tool_calls,
        annotations: None,
        function_call: state.function_call.clone(),
        audio: None,
    }
}

fn merge_function_call(state: &mut ChoiceState, delta: ChatCompletionFunctionCallDelta) {
    let entry = state.function_call.get_or_insert_with(|| ChatCompletionFunctionCall {
        name: String::new(),
        arguments: String::new(),
    });
    if let Some(name) = delta.name {
        entry.name = name;
    }
    if let Some(arguments) = delta.arguments {
        entry.arguments.push_str(&arguments);
    }
}

fn merge_tool_call(state: &mut ChoiceState, tool_call: ChatCompletionMessageToolCallChunk) {
    let index = tool_call.index;
    let entry = state.tool_calls.entry(index).or_insert_with(|| ToolCallState {
        id: tool_call.id.clone(),
        name: None,
        arguments: String::new(),
    });

    if tool_call.id.is_some() {
        entry.id = tool_call.id.clone();
    }

    if let Some(r#type) = tool_call.r#type
        && !matches!(r#type, ChatCompletionToolCallChunkType::Function) {
            return;
        }

    if let Some(function) = tool_call.function {
        if let Some(name) = function.name {
            entry.name = Some(name);
        }
        if let Some(arguments) = function.arguments {
            entry.arguments.push_str(&arguments);
        }
    }
}

fn merge_logprobs(target: &mut Option<ChatCompletionChoiceLogprobs>, incoming: ChatCompletionChoiceLogprobs) {
    let entry = target.get_or_insert_with(|| ChatCompletionChoiceLogprobs {
        content: None,
        refusal: None,
    });

    if let Some(mut content) = incoming.content {
        match entry.content.as_mut() {
            Some(existing) => existing.append(&mut content),
            None => entry.content = Some(content),
        }
    }

    if let Some(mut refusal) = incoming.refusal {
        match entry.refusal.as_mut() {
            Some(existing) => existing.append(&mut refusal),
            None => entry.refusal = Some(refusal),
        }
    }
}
