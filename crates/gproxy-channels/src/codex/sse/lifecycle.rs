use std::collections::BTreeMap;

use gproxy_protocol::openai::common::{ResponseItemLifecycleStatus, ResponseStreamEventTypeKnown};
use gproxy_protocol::openai::generate_content::responses::{
    KnownResponseStreamEvent, ResponseItem, ResponseMessageItem, ResponseMessageItemType,
    ResponseMessageOutputContentPart, ResponseOutputMessageItem, ResponseOutputMessageRole,
    ResponseOutputText, ResponseOutputTextType, ResponseReasoningTextPart, ResponseStreamEvent,
    TypedResponseItem,
};

use super::item_state::clear_started_payload;

#[derive(Default)]
pub(super) struct Lifecycle {
    items: BTreeMap<u32, ItemState>,
    terminal: bool,
}

#[derive(Default)]
struct ItemState {
    item: Option<ResponseItem>,
    added: bool,
    input_done: bool,
    done: bool,
    input: String,
    input_kind: Option<InputKind>,
    item_id: Option<String>,
    call_id: Option<String>,
    name: Option<String>,
}

#[derive(Clone, Copy)]
enum InputKind {
    Function,
    Custom,
}

impl Lifecycle {
    pub(super) fn normalize(
        &mut self,
        mut event: ResponseStreamEvent,
    ) -> Result<Vec<ResponseStreamEvent>, String> {
        let ResponseStreamEvent::Known(known) = &mut event else {
            return Ok(vec![event]);
        };
        match known.type_ {
            ResponseStreamEventTypeKnown::ResponseOutputItemAdded => self.note_item(known, true),
            ResponseStreamEventTypeKnown::ResponseOutputItemDone => {
                self.note_item(known, false);
                if let Some(index) = known.output_index {
                    let state = self.items.entry(index).or_default();
                    let mut output = Vec::new();
                    if let Some(item) = state.item.clone()
                        && !state.added
                    {
                        output.push(item_event(
                            ResponseStreamEventTypeKnown::ResponseOutputItemAdded,
                            index,
                            item,
                        ));
                        state.added = true;
                    }
                    if state.input_kind.is_some() && !state.input_done {
                        output.push(input_done_event(index, state));
                        state.input_done = true;
                    }
                    state.done = true;
                    output.push(event);
                    return Ok(output);
                }
            }
            ResponseStreamEventTypeKnown::ResponseOutputTextDelta => {
                self.push_text(known, false);
                if let Some(added) = self.added_event(known.output_index) {
                    return Ok(vec![added, event]);
                }
            }
            ResponseStreamEventTypeKnown::ResponseReasoningTextDelta => {
                self.push_text(known, true);
                if let Some(added) = self.added_event(known.output_index) {
                    return Ok(vec![added, event]);
                }
            }
            ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDelta => {
                self.push_input(known, InputKind::Function, false)
            }
            ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDone => {
                self.push_input(known, InputKind::Function, true)
            }
            ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDelta => {
                self.push_input(known, InputKind::Custom, false)
            }
            ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDone => {
                self.push_input(known, InputKind::Custom, true)
            }
            ResponseStreamEventTypeKnown::ResponseCompleted
            | ResponseStreamEventTypeKnown::ResponseIncomplete
            | ResponseStreamEventTypeKnown::ResponseFailed => {
                self.terminal = true;
                let mut output = self.repair()?;
                if let Some(response) = known.response.as_mut()
                    && response.output.is_empty()
                {
                    response.output = self.completed_items();
                }
                output.push(event);
                return Ok(output);
            }
            _ => {}
        }
        Ok(vec![event])
    }

    pub(super) fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn note_item(&mut self, event: &KnownResponseStreamEvent, added: bool) {
        let (Some(index), Some(item)) = (event.output_index, event.item.as_deref()) else {
            return;
        };
        let state = self.items.entry(index).or_default();
        state.item = Some(item.clone());
        state.item_id = item_id(item).or_else(|| event.item_id.clone());
        if added {
            state.added = true;
        }
        state.note_typed_item();
    }

    fn push_text(&mut self, event: &KnownResponseStreamEvent, reasoning: bool) {
        let (Some(index), Some(id), Some(delta)) = (
            event.output_index,
            event.item_id.as_deref(),
            event.delta.as_deref(),
        ) else {
            return;
        };
        let state = self.items.entry(index).or_default();
        state.item_id.get_or_insert_with(|| id.into());
        match state.item.as_mut() {
            Some(ResponseItem::Message(ResponseMessageItem::Output(message))) if !reasoning => {
                append_message(message, delta)
            }
            Some(ResponseItem::Typed(item)) if reasoning => {
                if let TypedResponseItem::Reasoning { content, .. } = item.as_mut() {
                    append_reasoning(content, delta);
                }
            }
            None if reasoning => {
                state.item = Some(reasoning_item(id, delta));
            }
            None => state.item = Some(message_item(id, delta)),
            _ => {}
        }
    }

    fn added_event(&mut self, index: Option<u32>) -> Option<ResponseStreamEvent> {
        let index = index?;
        let state = self.items.get_mut(&index)?;
        if state.added {
            return None;
        }
        let mut item = state.item.clone()?;
        clear_started_payload(&mut item);
        state.added = true;
        Some(item_event(
            ResponseStreamEventTypeKnown::ResponseOutputItemAdded,
            index,
            item,
        ))
    }

    fn push_input(&mut self, event: &KnownResponseStreamEvent, kind: InputKind, done: bool) {
        let Some(index) = event.output_index else {
            return;
        };
        let state = self.items.entry(index).or_default();
        state.input_kind = Some(kind);
        state.item_id = event.item_id.clone().or_else(|| state.item_id.take());
        state.name = event.name.clone().or_else(|| state.name.take());
        if done {
            let value = match kind {
                InputKind::Function => event.arguments.as_deref(),
                InputKind::Custom => event.input.as_deref(),
            };
            if let Some(value) = value {
                value.clone_into(&mut state.input);
            }
            state.input_done = true;
        } else if let Some(delta) = event.delta.as_deref() {
            state.input.push_str(delta);
        }
        state.apply_input();
    }

    fn repair(&mut self) -> Result<Vec<ResponseStreamEvent>, String> {
        let mut output = Vec::new();
        for (index, state) in &mut self.items {
            state.ensure_item()?;
            let Some(item) = state.item.clone() else {
                continue;
            };
            if !state.added {
                output.push(item_event(
                    ResponseStreamEventTypeKnown::ResponseOutputItemAdded,
                    *index,
                    item.clone(),
                ));
                state.added = true;
            }
            if state.input_kind.is_some() && !state.input_done {
                output.push(input_done_event(*index, state));
                state.input_done = true;
            }
            if !state.done {
                state.complete_item();
                let item = state.item.clone().expect("repaired item remains available");
                output.push(item_event(
                    ResponseStreamEventTypeKnown::ResponseOutputItemDone,
                    *index,
                    item,
                ));
                state.done = true;
            }
        }
        Ok(output)
    }

    fn completed_items(&self) -> Vec<ResponseItem> {
        self.items
            .values()
            .filter_map(|state| state.item.clone())
            .collect()
    }
}

impl ItemState {
    fn note_typed_item(&mut self) {
        let Some(ResponseItem::Typed(item)) = self.item.as_ref() else {
            return;
        };
        match item.as_ref() {
            TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                ..
            } => {
                self.input_kind = Some(InputKind::Function);
                arguments.clone_into(&mut self.input);
                self.call_id = Some(call_id.clone());
                self.name = Some(name.clone());
            }
            TypedResponseItem::CustomToolCall {
                input,
                call_id,
                name,
                ..
            } => {
                self.input_kind = Some(InputKind::Custom);
                input.clone_into(&mut self.input);
                self.call_id = Some(call_id.clone());
                self.name = Some(name.clone());
            }
            _ => {}
        }
    }

    fn apply_input(&mut self) {
        let Some(ResponseItem::Typed(item)) = self.item.as_mut() else {
            return;
        };
        match item.as_mut() {
            TypedResponseItem::FunctionCall { arguments, .. } => self.input.clone_into(arguments),
            TypedResponseItem::CustomToolCall { input, .. } => self.input.clone_into(input),
            _ => {}
        }
    }

    fn complete_item(&mut self) {
        let Some(item) = self.item.as_mut() else {
            return;
        };
        match item {
            ResponseItem::Message(ResponseMessageItem::Output(message)) => {
                message.status = ResponseItemLifecycleStatus::Completed
            }
            ResponseItem::Typed(item) => match item.as_mut() {
                TypedResponseItem::FunctionCall { status, .. }
                | TypedResponseItem::Reasoning { status, .. } => {
                    *status = Some(ResponseItemLifecycleStatus::Completed)
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn ensure_item(&mut self) -> Result<(), String> {
        if self.item.is_some() {
            self.apply_input();
            return Ok(());
        }
        let id = self
            .item_id
            .clone()
            .ok_or_else(|| "Codex tool stream item id is missing".to_owned())?;
        let name = self
            .name
            .clone()
            .ok_or_else(|| "Codex tool stream name is missing".to_owned())?;
        let call_id = self
            .call_id
            .clone()
            .ok_or_else(|| "Codex tool stream call_id is missing".to_owned())?;
        let item = match self.input_kind {
            Some(InputKind::Function) => TypedResponseItem::FunctionCall {
                arguments: self.input.clone(),
                call_id,
                name,
                id: Some(id),
                caller: None,
                namespace: None,
                status: Some(ResponseItemLifecycleStatus::InProgress),
                rest: Default::default(),
            },
            Some(InputKind::Custom) => TypedResponseItem::CustomToolCall {
                call_id,
                input: self.input.clone(),
                name,
                id: Some(id),
                caller: None,
                namespace: None,
                rest: Default::default(),
            },
            None => return Ok(()),
        };
        self.item = Some(ResponseItem::Typed(Box::new(item)));
        Ok(())
    }
}

fn message_item(id: &str, text: &str) -> ResponseItem {
    ResponseItem::Message(ResponseMessageItem::Output(ResponseOutputMessageItem {
        type_: ResponseMessageItemType::Message,
        id: Some(id.into()),
        role: ResponseOutputMessageRole::Assistant,
        content: vec![ResponseMessageOutputContentPart::OutputText(
            ResponseOutputText {
                type_: ResponseOutputTextType::OutputText,
                annotations: Vec::new(),
                logprobs: None,
                text: text.into(),
                rest: Default::default(),
            },
        )],
        status: ResponseItemLifecycleStatus::InProgress,
        phase: None,
        rest: Default::default(),
    }))
}

fn reasoning_item(id: &str, text: &str) -> ResponseItem {
    ResponseItem::Typed(Box::new(TypedResponseItem::Reasoning {
        id: Some(id.into()),
        summary: Vec::new(),
        content: Some(vec![ResponseReasoningTextPart {
            text: text.into(),
            type_: "reasoning_text".into(),
            rest: Default::default(),
        }]),
        encrypted_content: None,
        status: Some(ResponseItemLifecycleStatus::InProgress),
        rest: Default::default(),
    }))
}

fn append_message(message: &mut ResponseOutputMessageItem, delta: &str) {
    if let Some(ResponseMessageOutputContentPart::OutputText(text)) = message.content.last_mut() {
        text.text.push_str(delta);
    } else {
        message
            .content
            .push(ResponseMessageOutputContentPart::OutputText(
                ResponseOutputText {
                    type_: ResponseOutputTextType::OutputText,
                    annotations: Vec::new(),
                    logprobs: None,
                    text: delta.into(),
                    rest: Default::default(),
                },
            ));
    }
}

fn append_reasoning(content: &mut Option<Vec<ResponseReasoningTextPart>>, delta: &str) {
    let parts = content.get_or_insert_with(Vec::new);
    if let Some(part) = parts.last_mut() {
        part.text.push_str(delta);
    } else {
        parts.push(ResponseReasoningTextPart {
            text: delta.into(),
            type_: "reasoning_text".into(),
            rest: Default::default(),
        });
    }
}

fn item_event(
    kind: ResponseStreamEventTypeKnown,
    index: u32,
    item: ResponseItem,
) -> ResponseStreamEvent {
    let mut event = empty_event(kind);
    event.output_index = Some(index);
    event.item = Some(Box::new(item));
    ResponseStreamEvent::Known(Box::new(event))
}

fn input_done_event(index: u32, state: &ItemState) -> ResponseStreamEvent {
    let kind = match state.input_kind {
        Some(InputKind::Function) => {
            ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDone
        }
        Some(InputKind::Custom) => ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDone,
        None => unreachable!("input done requires a tool kind"),
    };
    let mut event = empty_event(kind);
    event.output_index = Some(index);
    event.item_id = state.item_id.clone();
    event.name = state.name.clone();
    match state.input_kind {
        Some(InputKind::Function) => event.arguments = Some(state.input.clone()),
        Some(InputKind::Custom) => event.input = Some(state.input.clone()),
        None => {}
    }
    ResponseStreamEvent::Known(Box::new(event))
}

fn empty_event(kind: ResponseStreamEventTypeKnown) -> KnownResponseStreamEvent {
    KnownResponseStreamEvent {
        type_: kind,
        sequence_number: None,
        response: None,
        item: None,
        output_index: None,
        content_index: None,
        item_id: None,
        part: None,
        delta: None,
        logprobs: None,
        text: None,
        annotation: None,
        annotation_index: None,
        arguments: None,
        name: None,
        input: None,
        refusal: None,
        summary_index: None,
        partial_image_b64: None,
        partial_image_index: None,
        code: None,
        message: None,
        param: None,
        reasoning_part: None,
        rest: Default::default(),
    }
}

fn item_id(item: &ResponseItem) -> Option<String> {
    match item {
        ResponseItem::Message(ResponseMessageItem::Output(message)) => message.id.clone(),
        ResponseItem::Message(ResponseMessageItem::Input(message)) => message.id.clone(),
        ResponseItem::Typed(item) => match item.as_ref() {
            TypedResponseItem::FunctionCall { id, .. }
            | TypedResponseItem::CustomToolCall { id, .. }
            | TypedResponseItem::Reasoning { id, .. }
            | TypedResponseItem::ApplyPatchCall { id, .. }
            | TypedResponseItem::ShellCall { id, .. } => id.clone(),
            _ => None,
        },
        _ => None,
    }
}
