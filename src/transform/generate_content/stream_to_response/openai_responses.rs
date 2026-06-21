use std::collections::BTreeMap;

use crate::protocol::openai;

use super::super::common;

pub fn response(
    events: impl IntoIterator<Item = openai::ResponseStreamEvent>,
) -> openai::ResponseObject {
    let mut collector = ResponseCollector::default();
    for event in events {
        collector.push(event);
    }
    collector.finish()
}

#[derive(Default)]
struct ResponseCollector {
    response: Option<openai::ResponseObject>,
    output: BTreeMap<u32, OutputState>,
    error: Option<openai::ResponseError>,
}

impl ResponseCollector {
    fn push(&mut self, event: openai::ResponseStreamEvent) {
        let openai::ResponseStreamEvent::Known(event) = event else {
            return;
        };

        match event {
            openai::KnownResponseStreamEvent::ResponseCreated { response, .. }
            | openai::KnownResponseStreamEvent::ResponseInProgress { response, .. }
            | openai::KnownResponseStreamEvent::ResponseCompleted { response, .. }
            | openai::KnownResponseStreamEvent::ResponseFailed { response, .. }
            | openai::KnownResponseStreamEvent::ResponseIncomplete { response, .. }
            | openai::KnownResponseStreamEvent::ResponseQueued { response, .. } => {
                self.remember_response(*response);
            }
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
                item,
                output_index,
                ..
            } => {
                self.output_state(output_index).seed_item(item.0, false);
            }
            openai::KnownResponseStreamEvent::ResponseOutputItemDone {
                item, output_index, ..
            } => {
                self.output_state(output_index).seed_item(item.0, true);
            }
            openai::KnownResponseStreamEvent::ResponseContentPartAdded {
                content_index,
                item_id,
                output_index,
                part,
                ..
            } => {
                self.output_state(output_index).push_content_part(
                    content_index,
                    item_id,
                    part,
                    false,
                );
            }
            openai::KnownResponseStreamEvent::ResponseContentPartDone {
                content_index,
                item_id,
                output_index,
                part,
                ..
            } => {
                self.output_state(output_index).push_content_part(
                    content_index,
                    item_id,
                    part,
                    true,
                );
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
                content_index,
                delta,
                item_id,
                logprobs,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                let part = state.message.text_part(content_index);
                part.push_delta(delta);
                part.push_logprobs(logprobs);
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDone {
                content_index,
                item_id,
                logprobs,
                output_index,
                text,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                let part = state.message.text_part(content_index);
                part.set_done(text);
                part.set_logprobs(logprobs);
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextAnnotationAdded {
                annotation,
                annotation_index,
                content_index,
                item_id,
                output_index,
                ..
            } => {
                if let Ok(annotation) =
                    serde_json::from_value::<openai::ResponseAnnotation>(annotation)
                {
                    let state = self.output_state(output_index);
                    state.message.id.get_or_insert(item_id);
                    state
                        .message
                        .text_part(content_index)
                        .push_annotation(annotation_index, annotation);
                }
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                state.message.refusal_part(content_index).push_delta(delta);
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDone {
                content_index,
                item_id,
                output_index,
                refusal,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                state.message.refusal_part(content_index).set_done(refusal);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartAdded {
                item_id,
                output_index,
                part,
                summary_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .summary_part(summary_index)
                    .push_delta(part.text);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartDone {
                item_id,
                output_index,
                part,
                summary_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .summary_part(summary_index)
                    .set_done(part.text);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta {
                delta,
                item_id,
                output_index,
                summary_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .summary_part(summary_index)
                    .push_delta(delta);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDone {
                item_id,
                output_index,
                summary_index,
                text,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state.reasoning.summary_part(summary_index).set_done(text);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .content_part(content_index)
                    .push_delta(delta);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDone {
                content_index,
                item_id,
                output_index,
                text,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state.reasoning.content_part(content_index).set_done(text);
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.function_call.item_id.get_or_insert(item_id);
                state.function_call.arguments.push_str(&delta);
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone {
                arguments,
                item_id,
                name,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.function_call.item_id.get_or_insert(item_id);
                state.function_call.name = Some(name);
                state.function_call.done_arguments = Some(arguments);
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.custom_tool_call.item_id.get_or_insert(item_id);
                state.custom_tool_call.input.push_str(&delta);
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone {
                input,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.custom_tool_call.item_id.get_or_insert(item_id);
                state.custom_tool_call.done_input = Some(input);
            }
            openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallCodeDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                self.output_state(output_index)
                    .push_code_interpreter_code(item_id, delta, false);
            }
            openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallCodeDone {
                code,
                item_id,
                output_index,
                ..
            } => {
                self.output_state(output_index)
                    .push_code_interpreter_code(item_id, code, true);
            }
            openai::KnownResponseStreamEvent::ResponseMcpCallArgumentsDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                self.output_state(output_index)
                    .push_mcp_arguments(item_id, delta, false);
            }
            openai::KnownResponseStreamEvent::ResponseMcpCallArgumentsDone {
                arguments,
                item_id,
                output_index,
                ..
            } => {
                self.output_state(output_index)
                    .push_mcp_arguments(item_id, arguments, true);
            }
            openai::KnownResponseStreamEvent::Error { code, message, .. } => {
                self.error = Some(openai::ResponseError {
                    code: openai::ResponseErrorCode::Unknown(code),
                    message,
                    extra: Default::default(),
                });
            }
            _ => {}
        }
    }

    fn remember_response(&mut self, mut response: openai::ResponseObject) {
        response.extra = Default::default();
        self.response = Some(response);
    }

    fn output_state(&mut self, index: u32) -> &mut OutputState {
        self.output
            .entry(index)
            .or_insert_with(|| OutputState::new(index))
    }

    fn finish(self) -> openai::ResponseObject {
        let mut response = self.response.unwrap_or_else(empty_response);
        let output = self
            .output
            .into_values()
            .filter_map(OutputState::finish)
            .collect::<Vec<_>>();

        if !output.is_empty() {
            response.output = output;
        }

        if let Some(output_text) = output_text(&response.output) {
            response.output_text = Some(output_text);
        }

        if let Some(error) = self.error {
            response.error = Some(error);
            response.status = Some(openai::ResponseStatus::Failed);
        } else if response.status.is_none() {
            response.status = Some(openai::ResponseStatus::Completed);
        }

        response.extra = Default::default();
        response
    }
}

struct OutputState {
    final_item: Option<openai::ResponseItem>,
    message: MessageState,
    reasoning: ReasoningState,
    function_call: FunctionCallState,
    custom_tool_call: CustomToolCallState,
    fallback_item: Option<openai::ResponseItem>,
}

impl OutputState {
    fn new(index: u32) -> Self {
        Self {
            final_item: None,
            message: MessageState::new(index),
            reasoning: ReasoningState::new(index),
            function_call: FunctionCallState::new(index),
            custom_tool_call: CustomToolCallState::new(index),
            fallback_item: None,
        }
    }

    fn seed_item(&mut self, item: openai::ResponseItem, final_item: bool) {
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
                self.reasoning.id = Some(id);
                self.reasoning.seed_summary(summary);
                self.reasoning.seed_content(content.unwrap_or_default());
                self.reasoning.encrypted_content = encrypted_content;
                self.reasoning.status = status;
            }
            item => self.fallback_item = Some(item),
        }
    }

    fn push_content_part(
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

    fn push_code_interpreter_code(&mut self, item_id: String, value: String, done: bool) {
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

    fn push_mcp_arguments(&mut self, item_id: String, value: String, done: bool) {
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

struct MessageState {
    index: u32,
    id: Option<String>,
    status: Option<openai::ResponseItemLifecycleStatus>,
    text: BTreeMap<u32, TextPartState>,
    refusal: BTreeMap<u32, TextPartState>,
}

impl MessageState {
    fn new(index: u32) -> Self {
        Self {
            index,
            id: None,
            status: None,
            text: BTreeMap::new(),
            refusal: BTreeMap::new(),
        }
    }

    fn text_part(&mut self, index: u32) -> &mut TextPartState {
        self.text.entry(index).or_default()
    }

    fn refusal_part(&mut self, index: u32) -> &mut TextPartState {
        self.refusal.entry(index).or_default()
    }

    fn seed_content(&mut self, parts: Vec<openai::ResponseMessageOutputContentPart>) {
        for (index, part) in parts.into_iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            match part {
                openai::ResponseMessageOutputContentPart::OutputText {
                    annotations,
                    logprobs,
                    text,
                    ..
                } => {
                    let part = self.text_part(index);
                    part.set_done(text);
                    part.seed_annotations(annotations);
                    part.logprobs = logprobs.unwrap_or_default();
                }
                openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
                    self.refusal_part(index).set_done(refusal);
                }
            }
        }
    }

    fn has_content(&self) -> bool {
        !self.text.is_empty() || !self.refusal.is_empty()
    }

    fn finish(self) -> openai::ResponseItem {
        let mut content = Vec::new();
        content.extend(
            self.text
                .into_values()
                .filter_map(TextPartState::finish_text),
        );
        content.extend(self.refusal.into_values().filter_map(|part| {
            non_empty(part.finish_plain()).map(|refusal| {
                openai::ResponseMessageOutputContentPart::Refusal {
                    refusal,
                    extra: Default::default(),
                }
            })
        }));

        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
            openai::ResponseOutputMessageItem {
                type_: openai::ResponseMessageItemType::Message,
                id: self.id.unwrap_or_else(|| format!("msg_{}", self.index)),
                role: openai::ResponseOutputMessageRole::Assistant,
                content,
                status: self
                    .status
                    .unwrap_or(openai::ResponseItemLifecycleStatus::Completed),
                phase: None,
                extra: Default::default(),
            },
        ))
    }
}

struct ReasoningState {
    index: u32,
    id: Option<String>,
    summary: BTreeMap<u32, TextPartState>,
    content: BTreeMap<u32, TextPartState>,
    encrypted_content: Option<String>,
    status: Option<openai::ResponseItemLifecycleStatus>,
}

impl ReasoningState {
    fn new(index: u32) -> Self {
        Self {
            index,
            id: None,
            summary: BTreeMap::new(),
            content: BTreeMap::new(),
            encrypted_content: None,
            status: None,
        }
    }

    fn summary_part(&mut self, index: u32) -> &mut TextPartState {
        self.summary.entry(index).or_default()
    }

    fn content_part(&mut self, index: u32) -> &mut TextPartState {
        self.content.entry(index).or_default()
    }

    fn seed_summary(&mut self, parts: Vec<openai::ResponseReasoningSummaryPart>) {
        for (index, part) in parts.into_iter().enumerate() {
            self.summary_part(u32::try_from(index).unwrap_or(u32::MAX))
                .set_done(part.text);
        }
    }

    fn seed_content(&mut self, parts: Vec<openai::ResponseReasoningTextPart>) {
        for (index, part) in parts.into_iter().enumerate() {
            self.content_part(u32::try_from(index).unwrap_or(u32::MAX))
                .set_done(part.text);
        }
    }

    fn has_content(&self) -> bool {
        !self.summary.is_empty() || !self.content.is_empty() || self.encrypted_content.is_some()
    }

    fn finish(self) -> openai::ResponseItem {
        let summary = self
            .summary
            .into_values()
            .filter_map(|part| {
                non_empty(part.finish_plain()).map(|text| openai::ResponseReasoningSummaryPart {
                    text,
                    type_: openai::ResponseReasoningSummaryType::SummaryText,
                    extra: Default::default(),
                })
            })
            .collect::<Vec<_>>();
        let content = self
            .content
            .into_values()
            .filter_map(|part| {
                non_empty(part.finish_plain()).map(|text| openai::ResponseReasoningTextPart {
                    text,
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    extra: Default::default(),
                })
            })
            .collect::<Vec<_>>();

        openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            id: self
                .id
                .unwrap_or_else(|| format!("reasoning_{}", self.index)),
            summary,
            content: (!content.is_empty()).then_some(content),
            encrypted_content: self.encrypted_content,
            status: self
                .status
                .or(Some(openai::ResponseItemLifecycleStatus::Completed)),
            extra: Default::default(),
        })
    }
}

struct FunctionCallState {
    index: u32,
    item_id: Option<String>,
    call_id: Option<String>,
    name: Option<String>,
    namespace: Option<String>,
    status: Option<openai::ResponseItemLifecycleStatus>,
    arguments: String,
    done_arguments: Option<String>,
}

impl FunctionCallState {
    fn new(index: u32) -> Self {
        Self {
            index,
            item_id: None,
            call_id: None,
            name: None,
            namespace: None,
            status: None,
            arguments: String::new(),
            done_arguments: None,
        }
    }

    fn has_content(&self) -> bool {
        self.call_id.is_some()
            || self.item_id.is_some()
            || self.name.is_some()
            || !self.arguments.is_empty()
            || self.done_arguments.is_some()
    }

    fn finish(self) -> openai::ResponseItem {
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments: self.done_arguments.unwrap_or(self.arguments),
            call_id: fallback_call_id(self.index, self.call_id, self.item_id.as_deref()),
            name: self.name.unwrap_or_default(),
            id: self.item_id,
            namespace: self.namespace,
            status: self
                .status
                .or(Some(openai::ResponseItemLifecycleStatus::Completed)),
            extra: Default::default(),
        })
    }
}

struct CustomToolCallState {
    index: u32,
    item_id: Option<String>,
    call_id: Option<String>,
    name: Option<String>,
    namespace: Option<String>,
    input: String,
    done_input: Option<String>,
}

impl CustomToolCallState {
    fn new(index: u32) -> Self {
        Self {
            index,
            item_id: None,
            call_id: None,
            name: None,
            namespace: None,
            input: String::new(),
            done_input: None,
        }
    }

    fn has_content(&self) -> bool {
        self.call_id.is_some()
            || self.item_id.is_some()
            || self.name.is_some()
            || !self.input.is_empty()
            || self.done_input.is_some()
    }

    fn finish(self) -> openai::ResponseItem {
        openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id: fallback_call_id(self.index, self.call_id, self.item_id.as_deref()),
            input: self.done_input.unwrap_or(self.input),
            name: self.name.unwrap_or_default(),
            id: self.item_id,
            namespace: self.namespace,
            extra: Default::default(),
        })
    }
}

#[derive(Default)]
struct TextPartState {
    delta: String,
    done: Option<String>,
    logprobs: Vec<openai::TokenLogprob>,
    annotations: BTreeMap<u32, openai::ResponseAnnotation>,
}

impl TextPartState {
    fn push_delta(&mut self, value: String) {
        self.delta.push_str(&value);
    }

    fn set_done(&mut self, value: String) {
        self.done = Some(value);
    }

    fn push_logprobs(&mut self, value: Option<Vec<openai::StreamTokenLogprob>>) {
        self.logprobs
            .extend(value.unwrap_or_default().into_iter().map(stream_logprob));
    }

    fn set_logprobs(&mut self, value: Option<Vec<openai::StreamTokenLogprob>>) {
        if let Some(value) = value {
            self.logprobs = value.into_iter().map(stream_logprob).collect();
        }
    }

    fn push_annotation(&mut self, index: u32, value: openai::ResponseAnnotation) {
        self.annotations.insert(index, sanitize_annotation(value));
    }

    fn seed_annotations(&mut self, values: Vec<openai::ResponseAnnotation>) {
        self.annotations
            .extend(values.into_iter().enumerate().map(|(index, value)| {
                (
                    u32::try_from(index).unwrap_or(u32::MAX),
                    sanitize_annotation(value),
                )
            }));
    }

    fn finish_plain(self) -> String {
        self.done.unwrap_or(self.delta)
    }

    fn finish_text(self) -> Option<openai::ResponseMessageOutputContentPart> {
        let text = non_empty(self.done.unwrap_or(self.delta))?;
        Some(openai::ResponseMessageOutputContentPart::OutputText {
            annotations: self.annotations.into_values().collect(),
            logprobs: (!self.logprobs.is_empty()).then_some(self.logprobs),
            text,
            extra: Default::default(),
        })
    }
}

fn sanitize_item(item: openai::ResponseItem) -> openai::ResponseItem {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                openai::ResponseOutputMessageItem {
                    type_: message.type_,
                    id: message.id,
                    role: message.role,
                    content: message
                        .content
                        .into_iter()
                        .map(sanitize_message_content_part)
                        .collect(),
                    status: message.status,
                    phase: message.phase,
                    extra: Default::default(),
                },
            ))
        }
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            namespace,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            namespace,
            status,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            namespace,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            namespace,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            id,
            summary,
            content,
            encrypted_content,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            id,
            summary: summary
                .into_iter()
                .map(|part| openai::ResponseReasoningSummaryPart {
                    text: part.text,
                    type_: part.type_,
                    extra: Default::default(),
                })
                .collect(),
            content: content.map(|parts| {
                parts
                    .into_iter()
                    .map(|part| openai::ResponseReasoningTextPart {
                        text: part.text,
                        type_: part.type_,
                        extra: Default::default(),
                    })
                    .collect()
            }),
            encrypted_content,
            status,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            outputs,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            outputs,
            status,
            extra: Default::default(),
        }),
        openai::ResponseItem::Typed(openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            server_label,
            approval_request_id,
            error,
            output,
            status,
            ..
        }) => openai::ResponseItem::Typed(openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            server_label,
            approval_request_id,
            error,
            output,
            status,
            extra: Default::default(),
        }),
        item => item,
    }
}

fn sanitize_message_content_part(
    part: openai::ResponseMessageOutputContentPart,
) -> openai::ResponseMessageOutputContentPart {
    match part {
        openai::ResponseMessageOutputContentPart::OutputText {
            annotations,
            logprobs,
            text,
            ..
        } => openai::ResponseMessageOutputContentPart::OutputText {
            annotations: annotations.into_iter().map(sanitize_annotation).collect(),
            logprobs,
            text,
            extra: Default::default(),
        },
        openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
            openai::ResponseMessageOutputContentPart::Refusal {
                refusal,
                extra: Default::default(),
            }
        }
    }
}

fn stream_logprob(value: openai::StreamTokenLogprob) -> openai::TokenLogprob {
    openai::TokenLogprob {
        token: value.token,
        bytes: None,
        logprob: value.logprob,
        top_logprobs: value
            .top_logprobs
            .unwrap_or_default()
            .into_iter()
            .filter_map(|top| {
                Some(openai::TokenLogprobTop {
                    token: top.token?,
                    bytes: None,
                    logprob: top.logprob?,
                    extra: Default::default(),
                })
            })
            .collect(),
        extra: Default::default(),
    }
}

fn sanitize_annotation(value: openai::ResponseAnnotation) -> openai::ResponseAnnotation {
    match value {
        openai::ResponseAnnotation::FileCitation {
            file_id,
            filename,
            index,
            ..
        } => openai::ResponseAnnotation::FileCitation {
            file_id,
            filename,
            index,
            extra: Default::default(),
        },
        openai::ResponseAnnotation::UrlCitation {
            end_index,
            start_index,
            title,
            url,
            ..
        } => openai::ResponseAnnotation::UrlCitation {
            end_index,
            start_index,
            title,
            url,
            extra: Default::default(),
        },
        openai::ResponseAnnotation::ContainerFileCitation {
            container_id,
            end_index,
            file_id,
            filename,
            start_index,
            ..
        } => openai::ResponseAnnotation::ContainerFileCitation {
            container_id,
            end_index,
            file_id,
            filename,
            start_index,
            extra: Default::default(),
        },
        openai::ResponseAnnotation::FilePath { file_id, index, .. } => {
            openai::ResponseAnnotation::FilePath {
                file_id,
                index,
                extra: Default::default(),
            }
        }
    }
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
    non_empty(text)
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn fallback_call_id(index: u32, call_id: Option<String>, item_id: Option<&str>) -> String {
    call_id.unwrap_or_else(|| common::fallback_response_call_id(index, item_id))
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
        object: openai::ResponseObjectType::Response,
        output: Vec::new(),
        output_text: None,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
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
