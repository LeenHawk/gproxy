use serde_json::{Map, Value};

use super::helpers::resolve_location;
use super::mutation::{stamp_chat_location, stamp_response_location};
use super::selection::{
    chat_message_locations, chat_system_locations, response_message_locations,
    response_system_locations,
};

pub(super) fn apply_chat(
    root: &mut Map<String, Value>,
    target: &str,
    index: Option<i64>,
) -> Result<(), &'static str> {
    let messages = root
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("missing messages array")?;
    let locations = match target {
        "system" => chat_system_locations(messages),
        "message" => chat_message_locations(messages),
        _ => return Err("unsupported cache breakpoint target"),
    };
    let selected = resolve_location(&locations, index)?;
    let messages = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .ok_or("missing messages array")?;
    stamp_chat_location(messages, selected)
}

pub(super) fn apply_responses(
    root: &mut Map<String, Value>,
    target: &str,
    index: Option<i64>,
) -> Result<(), &'static str> {
    let locations = match target {
        "system" => response_system_locations(root),
        "message" => response_message_locations(root),
        _ => return Err("unsupported cache breakpoint target"),
    };
    let selected = resolve_location(&locations, index)?;
    stamp_response_location(root, selected)
}
