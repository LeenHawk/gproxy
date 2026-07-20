use std::collections::BTreeMap;

use crate::protocol::openai;

use super::super::super::common;
use super::message::MessageState;
use super::reasoning::ReasoningState;
use super::sanitize::sanitize_item;
use super::tool::{CustomToolCallState, FunctionCallState};

pub(super) struct OutputState {
    final_item: Option<openai::ResponseItem>,
    pub(super) message: MessageState,
    pub(super) reasoning: ReasoningState,
    pub(super) function_call: FunctionCallState,
    pub(super) custom_tool_call: CustomToolCallState,
    fallback_item: Option<openai::ResponseItem>,
}

impl OutputState {
    pub(super) fn new(index: u32) -> Self {
        Self {
            final_item: None,
            message: MessageState::new(index),
            reasoning: ReasoningState::new(index),
            function_call: FunctionCallState::new(index),
            custom_tool_call: CustomToolCallState::new(index),
            fallback_item: None,
        }
    }

    pub(super) fn seed_item(&mut self, item: openai::ResponseItem, final_item: bool) {
        if final_item {
            self.final_item = Some(item);
            return;
        }

        match item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                self.message.id = Some(message.id);
                self.message.status = Some(message.status);
                self.message.seed_content(message.content);
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                id,
                namespace,
                status,
                ..
            }) => {
                self.function_call.arguments = arguments;
                self.function_call.call_id = Some(call_id);
                self.function_call.name = Some(name);
                self.function_call.item_id = id;
                self.function_call.namespace = namespace;
                self.function_call.status = status;
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
                call_id,
                input,
                name,
                id,
                namespace,
                ..
            }) => {
                self.custom_tool_call.call_id = Some(call_id);
                self.custom_tool_call.input = input;
                self.custom_tool_call.name = Some(name);
                self.custom_tool_call.item_id = id;
                self.custom_tool_call.namespace = namespace;
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
                id,
                summary,
                content,
                encrypted_content,
                status,
                ..
            }) => {
                self.reasoning.id = id;
                self.reasoning.seed_summary(summary);
                self.reasoning.seed_content(content.unwrap_or_default());
                self.reasoning.encrypted_content = encrypted_content;
                self.reasoning.status = status;
            }
            item => self.fallback_item = Some(item),
        }
    }

    pub(super) fn push_content_part(
        &mut self,
        index: u32,
        item_id: String,
        part: openai::ResponseContentPart,
        done: bool,
    ) {
        match part {
            openai::ResponseContentPart::OutputText { text, .. } => {
                self.message.id.get_or_insert(item_id);
                if done {
                    self.message.text_part(index).set_done(text);
                } else {
                    self.message.text_part(index).push_delta(text);
                }
            }
            openai::ResponseContentPart::Refusal { refusal, .. } => {
                self.message.id.get_or_insert(item_id);
                if done {
                    self.message.refusal_part(index).set_done(refusal);
                } else {
                    self.message.refusal_part(index).push_delta(refusal);
                }
            }
            openai::ResponseContentPart::ReasoningText { text, .. } => {
                self.reasoning.id.get_or_insert(item_id);
                if done {
                    self.reasoning.content_part(index).set_done(text);
                } else {
                    self.reasoning.content_part(index).push_delta(text);
                }
            }
        }
    }

    pub(super) fn push_code_interpreter_code(
        &mut self,
        item_id: String,
        value: String,
        done: bool,
    ) {
        let item = self.fallback_item.get_or_insert_with(|| {
            openai::ResponseItem::Typed(openai::TypedResponseItem::CodeInterpreterCall {
                id: item_id.clone(),
                code: Some(String::new()),
                container_id: String::new(),
                outputs: None,
                status: openai::ResponseCodeInterpreterCallStatus::InProgress,
                extra: Default::default(),
            })
        });

        if let openai::ResponseItem::Typed(openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            status,
            ..
        }) = item
        {
            if id.is_empty() {
                *id = item_id;
            }
            if done {
                *code = Some(value);
                *status = openai::ResponseCodeInterpreterCallStatus::Completed;
            } else {
                code.get_or_insert_with(String::new).push_str(&value);
            }
        }
    }

    pub(super) fn push_mcp_arguments(&mut self, item_id: String, value: String, done: bool) {
        let item = self.fallback_item.get_or_insert_with(|| {
            openai::ResponseItem::Typed(openai::TypedResponseItem::McpCall {
                id: item_id.clone(),
                arguments: String::new(),
                name: String::new(),
                server_label: String::new(),
                approval_request_id: None,
                error: None,
                output: None,
                status: Some(openai::ResponseMcpCallStatus::InProgress),
                extra: Default::default(),
            })
        });

        if let openai::ResponseItem::Typed(openai::TypedResponseItem::McpCall {
            id,
            arguments,
            status,
            ..
        }) = item
        {
            if id.is_empty() {
                *id = item_id;
            }
            if done {
                *arguments = value;
                *status = Some(openai::ResponseMcpCallStatus::Completed);
            } else {
                arguments.push_str(&value);
            }
        }
    }

    fn finish(self) -> Option<openai::ResponseOutputItem> {
        if let Some(item) = self.final_item {
            return Some(openai::ResponseOutputItem(sanitize_item(item)));
        }
        if self.message.has_content() {
            return Some(openai::ResponseOutputItem(self.message.finish()));
        }
        if self.reasoning.has_content() {
            return Some(openai::ResponseOutputItem(self.reasoning.finish()));
        }
        if self.function_call.has_content() {
            return Some(openai::ResponseOutputItem(self.function_call.finish()));
        }
        if self.custom_tool_call.has_content() {
            return Some(openai::ResponseOutputItem(self.custom_tool_call.finish()));
        }
        self.fallback_item
            .map(sanitize_item)
            .map(openai::ResponseOutputItem)
    }
}

pub(super) fn finish_response(
    response: Option<openai::ResponseObject>,
    output: BTreeMap<u32, OutputState>,
    error: Option<openai::ResponseError>,
) -> openai::ResponseObject {
    let mut response = response.unwrap_or_else(empty_response);
    let output = output
        .into_values()
        .filter_map(OutputState::finish)
        .collect::<Vec<_>>();

    if !output.is_empty() {
        response.output = output;
    }
    if let Some(output_text) = output_text(&response.output) {
        response.output_text = Some(output_text);
    }
    if let Some(error) = error {
        response.error = Some(error);
        response.status = Some(openai::ResponseStatus::Failed);
    } else if response.status.is_none() {
        response.status = Some(openai::ResponseStatus::Completed);
    }
    response.extra = Default::default();
    response
}

fn output_text(output: &[openai::ResponseOutputItem]) -> Option<String> {
    let mut text = String::new();
    for item in output {
        if let openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) = &item.0
        {
            for part in &message.content {
                if let openai::ResponseMessageOutputContentPart::OutputText { text: part, .. } =
                    part
                {
                    text.push_str(part);
                }
            }
        }
    }
    (!text.is_empty()).then_some(text)
}

fn empty_response() -> openai::ResponseObject {
    openai::ResponseObject {
        id: String::new(),
        created_at: 0,
        background: None,
        completed_at: Some(0),
        conversation: None,
        error: None,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(common::default_openai_model()),
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output: Vec::new(),
        output_text: None,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: None,
        status: Some(openai::ResponseStatus::Completed),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: None,
        user: None,
        extra: Default::default(),
    }
}
