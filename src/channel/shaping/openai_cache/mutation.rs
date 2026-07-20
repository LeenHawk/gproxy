use serde_json::{Map, Value};

use super::schema::{
    chat_text_part, explicit_breakpoint, is_supported_chat_part, is_supported_response_part,
    response_input_text_part, response_message, response_message_with_role,
};
use super::selection::{ChatLocation, ResponsesLocation};

pub(super) fn stamp_chat_location(
    messages: &mut [Value],
    location: ChatLocation,
) -> Result<(), &'static str> {
    match location {
        ChatLocation::ContentString(message_index) => {
            let content = messages
                .get_mut(message_index)
                .and_then(|message| message.get_mut("content"))
                .ok_or("target content not found")?;
            let text = content
                .as_str()
                .ok_or("target content is not text")?
                .to_string();
            *content = Value::Array(vec![chat_text_part(text, true)]);
            Ok(())
        }
        ChatLocation::ContentPart(message_index, part_index) => {
            let part = messages
                .get_mut(message_index)
                .and_then(|message| message.get_mut("content"))
                .and_then(Value::as_array_mut)
                .and_then(|parts| parts.get_mut(part_index))
                .and_then(Value::as_object_mut)
                .ok_or("target content block not found")?;
            if !is_supported_chat_part(part) {
                return Err("target block does not support an OpenAI cache breakpoint");
            }
            part.entry("prompt_cache_breakpoint")
                .or_insert_with(explicit_breakpoint);
            Ok(())
        }
    }
}

pub(super) fn stamp_response_location(
    root: &mut Map<String, Value>,
    location: ResponsesLocation,
) -> Result<(), &'static str> {
    match location {
        ResponsesLocation::Instructions => prepend_instruction_anchor(root)
            .then_some(())
            .ok_or("unable to insert instructions cache anchor"),
        ResponsesLocation::InputString => {
            let text = root
                .get("input")
                .and_then(Value::as_str)
                .ok_or("input is not text")?
                .to_string();
            root.insert(
                "input".into(),
                Value::Array(vec![response_message(text, true)]),
            );
            Ok(())
        }
        ResponsesLocation::ContentString(item_index) => {
            let content = root
                .get_mut("input")
                .and_then(Value::as_array_mut)
                .and_then(|items| items.get_mut(item_index))
                .and_then(|message| message.get_mut("content"))
                .ok_or("target content not found")?;
            let text = content
                .as_str()
                .ok_or("target content is not text")?
                .to_string();
            *content = Value::Array(vec![response_input_text_part(text, true)]);
            Ok(())
        }
        ResponsesLocation::ContentPart(item_index, part_index) => {
            let part = root
                .get_mut("input")
                .and_then(Value::as_array_mut)
                .and_then(|items| items.get_mut(item_index))
                .and_then(|message| message.get_mut("content"))
                .and_then(Value::as_array_mut)
                .and_then(|parts| parts.get_mut(part_index))
                .and_then(Value::as_object_mut)
                .ok_or("target content block not found")?;
            if !is_supported_response_part(part) {
                return Err("target block does not support an OpenAI cache breakpoint");
            }
            part.entry("prompt_cache_breakpoint")
                .or_insert_with(explicit_breakpoint);
            Ok(())
        }
    }
}

pub(super) fn prepend_instruction_anchor(root: &mut Map<String, Value>) -> bool {
    let input = root.remove("input");
    let mut items = match input {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(text)) => vec![response_message(text, false)],
        Some(Value::Array(items)) => items,
        Some(other) => {
            root.insert("input".into(), other);
            return false;
        }
    };
    // `instructions` cannot carry content-part metadata. A whitespace developer
    // block immediately after it creates the same rendered-prefix boundary.
    items.insert(
        0,
        response_message_with_role("developer", " ".to_string(), true),
    );
    root.insert("input".into(), Value::Array(items));
    true
}
