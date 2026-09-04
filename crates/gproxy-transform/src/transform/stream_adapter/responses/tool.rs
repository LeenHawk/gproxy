//! Tool-call item state (function / custom) and its synthetic events.

use crate::protocol::openai::{
    Extra, KnownResponseStreamEvent as KnownEvent, ResponseItem, ResponseItemLifecycleStatus,
    ResponseOutputItem, ResponseStreamEvent, TypedResponseItem,
};

use super::known;

#[derive(Default)]
pub(super) struct ResponsesToolItemState {
    kind: Option<ResponsesToolKind>,
    pub(super) item_id: Option<String>,
    call_id: Option<String>,
    async_: Option<bool>,
    pub(super) name: Option<String>,
    output_index: Option<u32>,
    pub(super) input: String,
    pub(super) input_done: bool,
    pub(super) item_done: bool,
}

#[derive(Clone, Copy)]
pub(super) enum ResponsesToolKind {
    Function,
    Custom,
}

impl ResponsesToolItemState {
    pub(super) fn note_kind(&mut self, kind: ResponsesToolKind, index: u32) {
        self.kind.get_or_insert(kind);
        self.output_index.get_or_insert(index);
    }

    pub(super) fn note_item(&mut self, item: &TypedResponseItem) {
        match item {
            TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                id,
                async_,
                ..
            } => self.note_item_parts(id.as_deref(), call_id, name, arguments, *async_),
            TypedResponseItem::CustomToolCall {
                call_id,
                input,
                name,
                id,
                async_,
                ..
            } => self.note_item_parts(id.as_deref(), call_id, name, input, *async_),
            _ => {}
        }
    }

    fn note_item_parts(
        &mut self,
        id: Option<&str>,
        call_id: &str,
        name: &str,
        input: &str,
        async_: Option<bool>,
    ) {
        if self.item_id.is_none() {
            self.item_id = id.map(str::to_owned);
        }
        if self.call_id.is_none() {
            self.call_id = Some(call_id.to_owned());
        }
        if self.name.is_none() {
            self.name = Some(name.to_owned());
        }
        if self.input.is_empty() {
            self.input.push_str(input);
        }
        if self.async_.is_none() {
            self.async_ = async_;
        }
    }

    pub(super) fn note_event_item_id(&mut self, item_id: &str) {
        if self.item_id.is_none() {
            self.item_id = Some(item_id.to_owned());
        }
    }

    /// Backfill the delta/done event's `item_id` from the recorded item id.
    pub(super) fn rewrite_event_item_id(&self, item_id: &mut String) {
        if let Some(id) = self.item_id.as_deref()
            && item_id.as_str() != id
        {
            id.clone_into(item_id);
        }
    }

    pub(super) fn can_finish(&self) -> bool {
        // Sparse streams can begin directly with an arguments delta, which
        // carries neither call_id nor name. Preserve the arguments and close
        // the item with the accessors' stable fallbacks instead of dropping
        // the entire tool call at EOF.
        self.kind.is_some() && self.item_id.is_some()
    }

    pub(super) fn input_done_event(&self) -> ResponseStreamEvent {
        match self.kind.expect("tool kind checked by can_finish") {
            ResponsesToolKind::Function => known(KnownEvent::ResponseFunctionCallArgumentsDone {
                arguments: self.input.clone(),
                item_id: self.item_id().to_owned(),
                name: self.name().to_owned(),
                output_index: self.output_index(),
                sequence_number: None,
                extra: Extra::new(),
            }),
            ResponsesToolKind::Custom => known(KnownEvent::ResponseCustomToolCallInputDone {
                input: self.input.clone(),
                item_id: self.item_id().to_owned(),
                output_index: self.output_index(),
                sequence_number: None,
                extra: Extra::new(),
            }),
        }
    }

    pub(super) fn item_done_event(&self) -> ResponseStreamEvent {
        known(KnownEvent::ResponseOutputItemDone {
            item: Box::new(self.completed_item()),
            output_index: self.output_index(),
            sequence_number: None,
            extra: Extra::new(),
        })
    }

    pub(super) fn completed_item(&self) -> ResponseOutputItem {
        let item = match self.kind.expect("tool kind checked by can_finish") {
            ResponsesToolKind::Function => {
                crate::protocol::wire!(TypedResponseItem::FunctionCall {
                    arguments: self.input.clone(),
                    call_id: self.call_id().to_owned(),
                    name: self.name().to_owned(),
                    id: Some(self.item_id().to_owned()),
                    caller: None,
                    namespace: None,
                    async_: self.async_,
                    status: Some(ResponseItemLifecycleStatus::Completed),
                    extra: Extra::new(),
                })
            }
            ResponsesToolKind::Custom => {
                crate::protocol::wire!(TypedResponseItem::CustomToolCall {
                    call_id: self.call_id().to_owned(),
                    input: self.input.clone(),
                    name: self.name().to_owned(),
                    id: Some(self.item_id().to_owned()),
                    caller: None,
                    namespace: None,
                    async_: self.async_,
                    extra: Extra::new(),
                })
            }
        };
        ResponseOutputItem::new(ResponseItem::Typed(item))
    }

    fn item_id(&self) -> &str {
        self.item_id.as_deref().unwrap_or("item_0")
    }
    fn call_id(&self) -> &str {
        self.call_id.as_deref().unwrap_or("call_0")
    }
    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }
    fn output_index(&self) -> u32 {
        self.output_index.unwrap_or(0)
    }
}
