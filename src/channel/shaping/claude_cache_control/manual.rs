use serde_json::{Map, Value, json};

use super::helpers::existing_cache_breakpoint_count;
use super::selection::{CacheLocation, locations, resolve};
use super::{MAX_BREAKPOINTS, canonicalize_claude_body};

pub(super) fn apply(
    body: &mut Value,
    target: &str,
    index: Option<i64>,
    ttl: Option<&str>,
) -> Result<(), &'static str> {
    canonicalize_claude_body(body);
    let root = body.as_object_mut().ok_or("body not an object")?;
    let control = match ttl {
        Some(ttl) => json!({"type": "ephemeral", "ttl": ttl}),
        None => json!({"type": "ephemeral"}),
    };

    if matches!(target, "top_level" | "global") {
        if root.contains_key("cache_control") {
            return Ok(());
        }
        if existing_cache_breakpoint_count(root) >= MAX_BREAKPOINTS {
            return Err("Claude cache breakpoint limit reached");
        }
        root.insert("cache_control".into(), control);
        return Ok(());
    }

    let location = resolve(&locations(root, target)?, index)?;
    stamp(root, location, control)
}

fn stamp(
    root: &mut Map<String, Value>,
    location: CacheLocation,
    control: Value,
) -> Result<(), &'static str> {
    let existing_count = existing_cache_breakpoint_count(root);
    let block = match location {
        CacheLocation::Tool(index) => root
            .get_mut("tools")
            .and_then(Value::as_array_mut)
            .and_then(|tools| tools.get_mut(index)),
        CacheLocation::System(index) => root
            .get_mut("system")
            .and_then(Value::as_array_mut)
            .and_then(|blocks| blocks.get_mut(index)),
        CacheLocation::Message {
            message_index,
            block_index,
        } => root
            .get_mut("messages")
            .and_then(Value::as_array_mut)
            .and_then(|messages| messages.get_mut(message_index))
            .and_then(|message| message.get_mut("content"))
            .and_then(Value::as_array_mut)
            .and_then(|blocks| blocks.get_mut(block_index)),
    }
    .and_then(Value::as_object_mut)
    .ok_or("target cache block not found")?;

    if block.contains_key("cache_control") {
        return Ok(());
    }
    if existing_count >= MAX_BREAKPOINTS {
        return Err("Claude cache breakpoint limit reached");
    }
    block.insert("cache_control".into(), control);
    Ok(())
}
