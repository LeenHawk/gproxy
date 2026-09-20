mod content;
mod repair;

use super::{
    event::{Event, invalid},
    tools,
};
use crate::{channel::ChannelError, channels::codex::shape::tools::Aliases};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
struct ItemState {
    item: Option<Value>,
    added: bool,
    done: bool,
    input_done: bool,
    pending: Vec<Event>,
}

pub(super) struct Lifecycle {
    items: BTreeMap<u32, ItemState>,
    aliases: Aliases,
    pub terminal: bool,
}

impl Lifecycle {
    pub fn new(aliases: Aliases) -> Self {
        Self {
            items: BTreeMap::new(),
            aliases,
            terminal: false,
        }
    }

    pub fn normalize(&mut self, mut event: Event) -> Result<Vec<Event>, ChannelError> {
        if self.terminal {
            return Err(invalid("event after terminal response"));
        }
        let mut output = Vec::new();
        match event.kind.as_str() {
            "response.output_item.added" | "response.output_item.done" => {
                let index = event.index()?;
                let item = event
                    .item
                    .take()
                    .filter(Value::is_object)
                    .ok_or_else(|| invalid("output item missing"))?;
                let state = self.items.entry(index).or_default();
                if state.done {
                    return Err(invalid("item already completed"));
                }
                state.merge(item);
                if event.kind.ends_with(".done") {
                    output.extend(state.finish(index, &self.aliases, Some(event))?);
                } else if !state.is_alias(&self.aliases) {
                    state.added = true;
                    event.item = state.item.clone();
                    output.push(event);
                    output.append(&mut state.pending);
                }
            }
            _ if event.input_field().is_some() => {
                let index = event.index()?;
                let field = event.input_field().unwrap();
                let done = event.kind.ends_with(".done");
                let text = if done {
                    event.arguments.as_ref().or(event.input.as_ref())
                } else {
                    event.delta.as_ref()
                }
                .ok_or_else(|| invalid("tool input missing"))?;
                let state = self.items.entry(index).or_default();
                if state.done {
                    return Err(invalid("tool delta after item completion"));
                }
                let item = state.item.get_or_insert_with(|| json!({"type":if field == "arguments" { "function_call" } else { "custom_tool_call" }}));
                let expected = if field == "arguments" {
                    "function_call"
                } else {
                    "custom_tool_call"
                };
                if item.get("type").and_then(Value::as_str) != Some(expected) {
                    return Err(invalid("tool input event does not match its item"));
                }
                if let Some(id) = &event.item_id {
                    item["id"] = json!(id);
                }
                if let Some(name) = &event.name {
                    item["name"] = json!(name);
                }
                if let Some(call_id) = event.rest.get("call_id") {
                    item["call_id"] = call_id.clone();
                }
                if done {
                    item[field] = json!(text);
                    state.input_done = true;
                } else {
                    let value = item
                        .as_object_mut()
                        .unwrap()
                        .entry(field)
                        .or_insert_with(|| json!(""));
                    let current = value
                        .as_str()
                        .ok_or_else(|| invalid("tool input must be text"))?;
                    *value = json!(format!("{current}{text}"));
                }
                if event.item_id.is_none() {
                    event.item_id = item.get("id").and_then(Value::as_str).map(str::to_owned);
                }
                if done && field == "arguments" && event.name.is_none() {
                    event.name = item.get("name").and_then(Value::as_str).map(str::to_owned);
                }
                if state.is_alias(&self.aliases) { /* Native inputs are emitted atomically at item completion. */
                } else if state.added {
                    output.push(event);
                } else {
                    state.pending.push(event);
                }
            }
            "response.completed" | "response.incomplete" | "response.failed" => {
                let response = event
                    .response
                    .as_mut()
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| invalid("terminal response missing"))?;
                let explicit = match response.get("output") {
                    Some(Value::Array(items)) => items.clone(),
                    None | Some(Value::Null) => Vec::new(),
                    _ => return Err(invalid("response.output must be an array")),
                };
                for (index, item) in explicit.into_iter().enumerate() {
                    let index = u32::try_from(index).map_err(invalid)?;
                    if !item.is_object() {
                        return Err(invalid("response output item must be an object"));
                    }
                    self.items.entry(index).or_default().merge(item);
                }
                for (index, state) in &mut self.items {
                    if !state.done {
                        if event.kind == "response.completed" {
                            output.extend(state.finish(*index, &self.aliases, None)?);
                        } else if let Some(item) = &mut state.item {
                            // A failed/incomplete response does not prove that a
                            // partial tool input is executable or even parseable.
                            item["status"] = json!("incomplete");
                        }
                    }
                }
                let restored: Result<Vec<_>, _> = self
                    .items
                    .values()
                    .filter_map(|state| state.item.clone())
                    .map(|item| tools::restore(&self.aliases, item))
                    .collect();
                response.insert("output".into(), Value::Array(restored?));
                self.terminal = true;
                output.push(event);
            }
            "error" => {
                self.terminal = true;
                output.push(event);
            }
            _ if content::handles(&event) => {
                let index = event.index()?;
                let state = self.items.entry(index).or_default();
                content::apply(state, &event)?;
                if !state.added {
                    let item = content::started(
                        state
                            .item
                            .clone()
                            .ok_or_else(|| invalid("content item missing"))?,
                    );
                    output.push(Event::item("response.output_item.added", index, item));
                    state.added = true;
                }
                output.push(event);
            }
            _ => output.push(event),
        }
        // Repair may add events; keep references consistent with mapped replay IDs.
        for event in &mut output {
            if let Some(id) = &mut event.item_id
                && let Some(original) = self.aliases.ids.get(id)
            {
                *id = original.clone();
            }
            if let Some(item) = &mut event.item
                && let Some(id) = item.get("id").and_then(Value::as_str)
                && let Some(original) = self.aliases.ids.get(id)
            {
                item["id"] = json!(original);
            }
        }
        Ok(output)
    }
}

impl ItemState {
    fn merge(&mut self, item: Value) {
        // Sparse final snapshots must not erase input collected from deltas.
        let mut merged = self.item.take().unwrap_or_else(|| json!({}));
        for (key, value) in item.as_object().unwrap() {
            if matches!(key.as_str(), "arguments" | "input")
                && value.as_str() == Some("")
                && merged
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
            {
                continue;
            }
            merged[key] = value.clone();
        }
        self.item = Some(merged);
    }
    fn is_alias(&self, aliases: &Aliases) -> bool {
        self.item
            .as_ref()
            .is_some_and(|item| tools::kind(aliases, item).is_some())
    }
}
