use crate::protocol::openai;

use super::super::super::common;

pub(super) struct FunctionCallState {
    index: u32,
    pub(super) item_id: Option<String>,
    pub(super) call_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) namespace: Option<String>,
    pub(super) status: Option<openai::ResponseItemLifecycleStatus>,
    pub(super) arguments: String,
    pub(super) done_arguments: Option<String>,
}

impl FunctionCallState {
    pub(super) fn new(index: u32) -> Self {
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

    pub(super) fn has_content(&self) -> bool {
        self.call_id.is_some()
            || self.item_id.is_some()
            || self.name.is_some()
            || !self.arguments.is_empty()
            || self.done_arguments.is_some()
    }

    pub(super) fn finish(self) -> openai::ResponseItem {
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments: self.done_arguments.unwrap_or(self.arguments),
            call_id: fallback_call_id(self.index, self.call_id, self.item_id.as_deref()),
            name: self.name.unwrap_or_default(),
            id: self.item_id,
            caller: None,
            namespace: self.namespace,
            status: self
                .status
                .or(Some(openai::ResponseItemLifecycleStatus::Completed)),
            extra: Default::default(),
        })
    }
}

pub(super) struct CustomToolCallState {
    index: u32,
    pub(super) item_id: Option<String>,
    pub(super) call_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) namespace: Option<String>,
    pub(super) input: String,
    pub(super) done_input: Option<String>,
}

impl CustomToolCallState {
    pub(super) fn new(index: u32) -> Self {
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

    pub(super) fn has_content(&self) -> bool {
        self.call_id.is_some()
            || self.item_id.is_some()
            || self.name.is_some()
            || !self.input.is_empty()
            || self.done_input.is_some()
    }

    pub(super) fn finish(self) -> openai::ResponseItem {
        openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id: fallback_call_id(self.index, self.call_id, self.item_id.as_deref()),
            input: self.done_input.unwrap_or(self.input),
            name: self.name.unwrap_or_default(),
            id: self.item_id,
            caller: None,
            namespace: self.namespace,
            extra: Default::default(),
        })
    }
}

fn fallback_call_id(index: u32, call_id: Option<String>, item_id: Option<&str>) -> String {
    call_id.unwrap_or_else(|| common::fallback_response_call_id(index, item_id))
}
