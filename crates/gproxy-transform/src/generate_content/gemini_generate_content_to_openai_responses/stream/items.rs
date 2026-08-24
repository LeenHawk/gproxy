use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::{State, ToolCall, events};

impl State {
    pub(super) fn item_added(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let item = *event.item;
        let item_id = item_id(&item).ok_or_else(|| {
            TransformError::shape("Responses stream", "output item id is missing")
        })?;
        if let openai::ResponseItem::Typed(item) = item {
            match *item {
                openai::TypedResponseItem::FunctionCall {
                    arguments,
                    call_id,
                    name,
                    rest,
                    ..
                } => {
                    self.calls.insert(
                        item_id.clone(),
                        ToolCall {
                            call_id,
                            name,
                            arguments,
                            custom: false,
                            rest,
                        },
                    );
                    self.call_indices.insert(event.output_index, item_id);
                }
                openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    input,
                    name,
                    rest,
                    ..
                } => {
                    self.calls.insert(
                        item_id.clone(),
                        ToolCall {
                            call_id,
                            name,
                            arguments: input,
                            custom: true,
                            rest,
                        },
                    );
                    self.call_indices.insert(event.output_index, item_id);
                }
                _ => {}
            }
        }
        Ok(Vec::new())
    }

    pub(super) fn item_done(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let item = *event.item;
        let key = item_id(&item).unwrap_or_else(|| format!("index:{}", event.output_index));
        self.emit_item(item, key)
    }

    pub(super) fn emit_item(
        &mut self,
        mut item: openai::ResponseItem,
        key: String,
    ) -> Result<Vec<Bytes>, TransformError> {
        if self.emitted.contains(&key) {
            return Ok(Vec::new());
        }
        if self.text_items.contains(&key) {
            item = match item {
                openai::ResponseItem::Message(_) => return Ok(Vec::new()),
                openai::ResponseItem::Typed(item) => match *item {
                    openai::TypedResponseItem::Reasoning {
                        id,
                        encrypted_content: Some(encrypted_content),
                        status,
                        rest,
                        ..
                    } => openai::ResponseItem::Typed(Box::new(
                        openai::TypedResponseItem::Reasoning {
                            id,
                            summary: Vec::new(),
                            content: None,
                            encrypted_content: Some(encrypted_content),
                            status,
                            rest,
                        },
                    )),
                    openai::TypedResponseItem::Reasoning { .. } => return Ok(Vec::new()),
                    other => openai::ResponseItem::Typed(Box::new(other)),
                },
                other => other,
            };
        }
        let content = self.content.item(item)?;
        self.emitted.insert(key);
        content
            .map(|content| {
                self.emit(events::chunk(
                    Some(content),
                    None,
                    None,
                    self.response_id.clone(),
                    self.model.clone(),
                ))
            })
            .transpose()
            .map(|value| value.into_iter().collect())
    }
}

pub(super) fn item_id(item: &openai::ResponseItem) -> Option<String> {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            message.id.clone()
        }
        openai::ResponseItem::Typed(item) => match item.as_ref() {
            openai::TypedResponseItem::FunctionCall { id, .. }
            | openai::TypedResponseItem::CustomToolCall { id, .. }
            | openai::TypedResponseItem::ShellCall { id, .. }
            | openai::TypedResponseItem::ApplyPatchCall { id, .. }
            | openai::TypedResponseItem::ShellCallOutput { id, .. }
            | openai::TypedResponseItem::ApplyPatchCallOutput { id, .. }
            | openai::TypedResponseItem::Reasoning { id, .. } => id.clone(),
            openai::TypedResponseItem::LocalShellCall { id, .. }
            | openai::TypedResponseItem::LocalShellCallOutput { id, .. } => Some(id.clone()),
            _ => None,
        },
        _ => None,
    }
}
