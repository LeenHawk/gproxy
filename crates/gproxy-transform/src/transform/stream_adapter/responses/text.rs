//! Text-bearing item state (message / reasoning) and its synthetic events.

use crate::protocol::openai::{
    Extra, KnownResponseStreamEvent as KnownEvent, ResponseContentPart, ResponseItem,
    ResponseItemLifecycleStatus, ResponseMessageItem, ResponseMessageItemType,
    ResponseMessageOutputContentPart, ResponseOutputItem, ResponseOutputMessageItem,
    ResponseOutputMessageRole, ResponseReasoningTextPart, ResponseReasoningTextType,
    ResponseStreamEvent, TypedResponseItem,
};

use super::known;

#[derive(Default)]
pub(super) struct ResponsesTextItemState {
    pub(super) started: bool,
    done: bool,
    id: Option<String>,
    output_index: Option<u32>,
    content_index: Option<u32>,
    pub(super) text: String,
}

impl ResponsesTextItemState {
    /// Record delta identity; emit the synthetic item_added on first delta.
    pub(super) fn ensure(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        build: impl FnOnce(&Self) -> ResponseStreamEvent,
    ) -> Vec<ResponseStreamEvent> {
        if self.id.is_none() {
            self.id = Some(item_id.to_owned());
        }
        self.output_index.get_or_insert(output_index);
        self.content_index.get_or_insert(content_index);
        if self.started {
            return Vec::new();
        }
        self.started = true;
        vec![build(self)]
    }

    pub(super) fn finish(
        &mut self,
        build: impl FnOnce(&Self) -> Vec<ResponseStreamEvent>,
    ) -> Vec<ResponseStreamEvent> {
        if !self.started || self.done {
            return Vec::new();
        }
        self.done = true;
        build(self)
    }

    pub(super) fn note_added(&mut self, id: Option<&str>, output_index: u32) {
        self.started = true;
        self.note_item_identity(id, output_index);
    }
    pub(super) fn note_item_done(&mut self, id: Option<&str>, output_index: u32) {
        self.done = true;
        self.note_item_identity(id, output_index);
    }
    pub(super) fn note_done_text(&mut self, text: &str) {
        self.done = true;
        text.clone_into(&mut self.text);
    }
    fn note_item_identity(&mut self, id: Option<&str>, output_index: u32) {
        if self.id.is_none() {
            self.id = id.map(str::to_owned);
        }
        self.output_index.get_or_insert(output_index);
    }
    pub(super) fn id(&self) -> &str {
        self.id.as_deref().unwrap_or("item_0")
    }
    pub(super) fn output_index(&self) -> u32 {
        self.output_index.unwrap_or(0)
    }
    pub(super) fn content_index(&self) -> u32 {
        self.content_index.unwrap_or(0)
    }
}

pub(super) fn message_item_added(state: &ResponsesTextItemState) -> ResponseStreamEvent {
    item_added(
        message_item(state, ResponseItemLifecycleStatus::InProgress),
        state.output_index(),
    )
}

pub(super) fn reasoning_item_added(state: &ResponsesTextItemState) -> ResponseStreamEvent {
    item_added(
        reasoning_item(state, ResponseItemLifecycleStatus::InProgress),
        state.output_index(),
    )
}

fn item_added(item: ResponseOutputItem, output_index: u32) -> ResponseStreamEvent {
    known(KnownEvent::ResponseOutputItemAdded {
        item: Box::new(item),
        output_index,
        sequence_number: None,
        extra: Extra::new(),
    })
}

/// Synthetic tail closing an open message item: output_text.done,
/// content_part.done, output_item.done.
pub(super) fn message_done_events(state: &ResponsesTextItemState) -> Vec<ResponseStreamEvent> {
    vec![
        known(KnownEvent::ResponseOutputTextDone {
            content_index: state.content_index(),
            item_id: state.id().to_owned(),
            logprobs: None,
            output_index: state.output_index(),
            sequence_number: None,
            text: state.text.clone(),
            extra: Extra::new(),
        }),
        known(KnownEvent::ResponseContentPartDone {
            content_index: state.content_index(),
            item_id: state.id().to_owned(),
            output_index: state.output_index(),
            part: ResponseContentPart::OutputText {
                annotations: Vec::new(),
                logprobs: None,
                text: state.text.clone(),
                extra: Extra::new(),
            },
            sequence_number: None,
            extra: Extra::new(),
        }),
        known(KnownEvent::ResponseOutputItemDone {
            item: Box::new(message_item(state, ResponseItemLifecycleStatus::Completed)),
            output_index: state.output_index(),
            sequence_number: None,
            extra: Extra::new(),
        }),
    ]
}

/// Synthetic tail closing an open reasoning item: reasoning_text.done,
/// output_item.done.
pub(super) fn reasoning_done_events(state: &ResponsesTextItemState) -> Vec<ResponseStreamEvent> {
    vec![
        known(KnownEvent::ResponseReasoningTextDone {
            content_index: state.content_index(),
            item_id: state.id().to_owned(),
            output_index: state.output_index(),
            sequence_number: None,
            text: state.text.clone(),
            extra: Extra::new(),
        }),
        known(KnownEvent::ResponseOutputItemDone {
            item: Box::new(reasoning_item(
                state,
                ResponseItemLifecycleStatus::Completed,
            )),
            output_index: state.output_index(),
            sequence_number: None,
            extra: Extra::new(),
        }),
    ]
}

pub(super) fn message_item(
    state: &ResponsesTextItemState,
    status: ResponseItemLifecycleStatus,
) -> ResponseOutputItem {
    ResponseOutputItem(ResponseItem::Message(ResponseMessageItem::Output(
        ResponseOutputMessageItem {
            type_: ResponseMessageItemType::Message,
            id: state.id().to_owned(),
            role: ResponseOutputMessageRole::Assistant,
            content: vec![ResponseMessageOutputContentPart::OutputText {
                annotations: Vec::new(),
                logprobs: None,
                text: state.text.clone(),
                extra: Extra::new(),
            }],
            status,
            phase: None,
            extra: Extra::new(),
        },
    )))
}

pub(super) fn reasoning_item(
    state: &ResponsesTextItemState,
    status: ResponseItemLifecycleStatus,
) -> ResponseOutputItem {
    ResponseOutputItem(ResponseItem::Typed(TypedResponseItem::Reasoning {
        id: Some(state.id().to_owned()),
        summary: Vec::new(),
        content: Some(vec![ResponseReasoningTextPart {
            text: state.text.clone(),
            type_: ResponseReasoningTextType::ReasoningText,
            extra: Extra::new(),
        }]),
        encrypted_content: None,
        status: Some(status),
        extra: Extra::new(),
    }))
}
