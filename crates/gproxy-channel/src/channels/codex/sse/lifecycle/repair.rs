use super::super::{
    event::{Event, invalid},
    tools,
};
use super::{ItemState, content};
use crate::{channel::ChannelError, channels::codex::shape::tools::Aliases};
use serde_json::{Value, json};

impl ItemState {
    pub(super) fn finish(
        &mut self,
        index: u32,
        aliases: &Aliases,
        original: Option<Event>,
    ) -> Result<Vec<Event>, ChannelError> {
        let item = self
            .item
            .as_mut()
            .ok_or_else(|| invalid("cannot recover missing item"))?;
        let field = match item.get("type").and_then(Value::as_str) {
            Some("function_call") => Some("arguments"),
            Some("custom_tool_call") => Some("input"),
            _ => None,
        };
        if field.is_some() {
            for key in ["id", "call_id", "name"] {
                if item.get(key).and_then(Value::as_str).is_none() {
                    return Err(invalid(format!("cannot recover tool {key}")));
                }
            }
        }
        if matches!(
            item.get("type").and_then(Value::as_str),
            Some("message" | "function_call" | "custom_tool_call" | "reasoning")
        ) && item
            .get("status")
            .and_then(Value::as_str)
            .is_none_or(|s| s == "in_progress")
        {
            item["status"] = json!("completed");
        }
        let alias = tools::kind(aliases, item).is_some();
        let restored = tools::restore(aliases, item.clone())?;
        let mut output = Vec::new();
        if !self.added {
            output.push(Event::item(
                "response.output_item.added",
                index,
                if alias {
                    restored.clone()
                } else {
                    content::started(restored.clone())
                },
            ));
            self.added = true;
        }
        if !alias {
            output.append(&mut self.pending);
            if !self.input_done
                && let Some(field) = field
            {
                let mut done = Event::new(if field == "arguments" {
                    "response.function_call_arguments.done"
                } else {
                    "response.custom_tool_call_input.done"
                });
                done.output_index = Some(index);
                done.item_id = item.get("id").and_then(Value::as_str).map(str::to_owned);
                let input = item
                    .get(field)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                if field == "arguments" {
                    done.arguments = Some(input);
                    done.name = item.get("name").and_then(Value::as_str).map(str::to_owned);
                } else {
                    done.input = Some(input);
                }
                output.push(done);
            }
        } else {
            self.pending.clear();
        }
        self.input_done = true;
        let mut done = original.unwrap_or_else(|| Event::new("response.output_item.done"));
        done.output_index = Some(index);
        done.item = Some(restored);
        output.push(done);
        self.done = true;
        Ok(output)
    }
}
