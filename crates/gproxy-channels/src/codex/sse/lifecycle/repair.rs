use gproxy_protocol::openai::generate_content::responses::{
    ResponseItem, ResponseMessageItem, ResponseOutputItemEvent, ResponseStreamEvent,
    TypedResponseItem,
};

use super::Lifecycle;
use super::content::{
    append_message, append_reasoning, clear_started_payload, item_id, message_item, reasoning_item,
};
use super::state::{InputKind, ItemState};
use crate::codex::sse::event;

impl Lifecycle {
    pub(super) fn note_item(&mut self, event: &ResponseOutputItemEvent, added: bool) {
        let state = self.items.entry(event.output_index).or_default();
        state.item = Some((*event.item).clone());
        state.item_id = item_id(&event.item);
        if added {
            state.added = true;
        }
        state.note_typed_item();
    }

    pub(super) fn push_text(
        &mut self,
        output_index: u32,
        item_id: &str,
        delta: &str,
        reasoning: bool,
    ) {
        let state = self.items.entry(output_index).or_default();
        state.item_id.get_or_insert_with(|| item_id.into());
        match state.item.as_mut() {
            Some(ResponseItem::Message(ResponseMessageItem::Output(message))) if !reasoning => {
                append_message(message, delta)
            }
            Some(ResponseItem::Typed(item)) if reasoning => {
                if let TypedResponseItem::Reasoning { content, .. } = item.as_mut() {
                    append_reasoning(content, delta);
                }
            }
            None if reasoning => state.item = Some(reasoning_item(item_id, delta)),
            None => state.item = Some(message_item(item_id, delta)),
            _ => {}
        }
    }

    pub(super) fn added_event(&mut self, output_index: u32) -> Option<ResponseStreamEvent> {
        let state = self.items.get_mut(&output_index)?;
        if state.added {
            return None;
        }
        let mut item = state.item.clone()?;
        clear_started_payload(&mut item);
        state.added = true;
        Some(event::output_item_added(
            output_index,
            item,
            None,
            Default::default(),
        ))
    }

    pub(super) fn push_input(
        &mut self,
        output_index: u32,
        kind: InputKind,
        item_id: Option<&str>,
        name: Option<&str>,
        value: &str,
        done: bool,
    ) {
        let state = self.items.entry(output_index).or_default();
        state.input_kind = Some(kind);
        if let Some(item_id) = item_id {
            state.item_id = Some(item_id.into());
        }
        if let Some(name) = name {
            state.name = Some(name.into());
        }
        if done {
            value.clone_into(&mut state.input);
            state.input_done = true;
        } else {
            state.input.push_str(value);
        }
        state.apply_input();
    }

    pub(super) fn repair(&mut self) -> Result<Vec<ResponseStreamEvent>, String> {
        let mut output = Vec::new();
        for (index, state) in &mut self.items {
            state.ensure_item()?;
            let Some(item) = state.item.clone() else {
                continue;
            };
            if !state.added {
                output.push(event::output_item_added(
                    *index,
                    item.clone(),
                    None,
                    Default::default(),
                ));
                state.added = true;
            }
            if state.input_kind.is_some() && !state.input_done {
                if let Some(done) = input_done_event(*index, state)? {
                    output.push(done);
                }
                state.input_done = true;
            }
            if !state.done {
                state.complete_item();
                let item = state.item.clone().expect("repaired item remains available");
                output.push(event::output_item_done(*index, item));
                state.done = true;
            }
        }
        Ok(output)
    }

    pub(super) fn completed_items(&self) -> Vec<ResponseItem> {
        self.items
            .values()
            .filter_map(|state| state.item.clone())
            .collect()
    }
}

pub(super) fn input_done_event(
    output_index: u32,
    state: &ItemState,
) -> Result<Option<ResponseStreamEvent>, String> {
    Ok(match state.input_kind {
        Some(InputKind::Function) => Some(event::function_arguments_done(
            output_index,
            state.item_id.clone(),
            state.name.clone(),
            state.input.clone(),
        )),
        Some(InputKind::Custom) => Some(event::custom_input_done(
            output_index,
            state
                .item_id
                .clone()
                .ok_or_else(|| "Codex custom tool stream item id is missing".to_owned())?,
            state.input.clone(),
        )),
        None => None,
    })
}
