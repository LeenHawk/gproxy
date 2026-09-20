use super::super::event::{Event, invalid};
use super::ItemState;
use crate::channel::ChannelError;
use serde_json::{Value, json};

pub(super) fn handles(event: &Event) -> bool {
    matches!(
        event.kind.as_str(),
        "response.output_text.delta"
            | "response.output_text.done"
            | "response.reasoning_text.delta"
            | "response.reasoning_text.done"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_summary_text.done"
            | "response.refusal.delta"
            | "response.refusal.done"
            | "response.content_part.added"
            | "response.content_part.done"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done"
    )
}

pub(super) fn apply(state: &mut ItemState, event: &Event) -> Result<(), ChannelError> {
    if state.done {
        return Err(invalid("content after item completion"));
    }
    let id = event
        .item_id
        .as_deref()
        .ok_or_else(|| invalid("content item_id missing"))?;
    let reasoning = event.kind.contains("reasoning_");
    let summary = event.kind.contains("summary_");
    let item = state.item.get_or_insert_with(|| {
        if reasoning {
            json!({"type":"reasoning","id":id,"summary":[],"status":"in_progress"})
        } else {
            json!({"type":"message","id":id,"role":"assistant","content":[],"status":"in_progress"})
        }
    });
    let key = if summary { "summary" } else { "content" };
    let index = event
        .rest
        .get(if summary {
            "summary_index"
        } else {
            "content_index"
        })
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let index = usize::try_from(index).map_err(invalid)?;
    if index > 4096 {
        return Err(invalid("content index exceeds repair limit"));
    }
    let parts = item
        .as_object_mut()
        .ok_or_else(|| invalid("content item must be an object"))?
        .entry(key)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| invalid("content must be an array"))?;
    let part_type = if summary {
        "summary_text"
    } else if reasoning {
        "reasoning_text"
    } else if event.kind.contains("refusal") {
        "refusal"
    } else {
        "output_text"
    };
    while parts.len() <= index {
        parts.push(match part_type {
            "output_text" => json!({"type":"output_text","text":"","annotations":[],"logprobs":[]}),
            "refusal" => json!({"type":"refusal","refusal":""}),
            _ => json!({"type":part_type,"text":""}),
        });
    }
    if let Some(part) = event.rest.get("part") {
        parts[index] = part.clone();
        return Ok(());
    }
    let field = if part_type == "refusal" {
        "refusal"
    } else {
        "text"
    };
    if let Some(delta) = &event.delta {
        let current = parts[index]
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or("");
        parts[index][field] = json!(format!("{current}{delta}"));
    } else if let Some(text) = event.rest.get(field) {
        parts[index][field] = text.clone();
    }
    Ok(())
}

pub(super) fn started(mut item: Value) -> Value {
    match item.get("type").and_then(Value::as_str) {
        Some("message") => {
            item["content"] = json!([]);
            item["status"] = json!("in_progress");
        }
        Some("reasoning") => {
            item["summary"] = json!([]);
            item["content"] = json!([]);
            item["status"] = json!("in_progress");
        }
        Some("function_call") => {
            item["arguments"] = json!("");
            item["status"] = json!("in_progress");
        }
        Some("custom_tool_call") => {
            item["input"] = json!("");
        }
        _ => {}
    }
    item
}
