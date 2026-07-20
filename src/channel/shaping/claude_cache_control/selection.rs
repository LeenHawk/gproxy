use serde_json::{Map, Value};

use super::schema::{is_cacheable_block, is_cacheable_message_block};

#[derive(Clone, Copy)]
pub(super) enum CacheLocation {
    Tool(usize),
    System(usize),
    Message {
        message_index: usize,
        block_index: usize,
    },
}

pub(super) fn locations(
    root: &Map<String, Value>,
    target: &str,
) -> Result<Vec<CacheLocation>, &'static str> {
    match target {
        "tools" => Ok(root
            .get("tools")
            .and_then(Value::as_array)
            .map(|tools| {
                tools
                    .iter()
                    .enumerate()
                    .filter(|(_, tool)| tool.is_object())
                    .map(|(index, _)| CacheLocation::Tool(index))
                    .collect()
            })
            .unwrap_or_default()),
        "system" => Ok(root
            .get("system")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, block)| block.as_object().is_some_and(is_cacheable_block))
                    .map(|(index, _)| CacheLocation::System(index))
                    .collect()
            })
            .unwrap_or_default()),
        "message" => Ok(message_locations(root)),
        _ => Err("unsupported cache breakpoint target"),
    }
}

fn message_locations(root: &Map<String, Value>) -> Vec<CacheLocation> {
    let mut locations = Vec::new();
    let Some(messages) = root.get("messages").and_then(Value::as_array) else {
        return locations;
    };
    for (message_index, message) in messages.iter().enumerate() {
        let role = message.get("role").and_then(Value::as_str);
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for (block_index, block) in blocks.iter().enumerate() {
            if block
                .as_object()
                .is_some_and(|block| is_cacheable_message_block(role, block))
            {
                locations.push(CacheLocation::Message {
                    message_index,
                    block_index,
                });
            }
        }
    }
    locations
}

pub(super) fn resolve(
    locations: &[CacheLocation],
    index: Option<i64>,
) -> Result<CacheLocation, &'static str> {
    let resolved = resolve_block_index(locations.len(), index)
        .ok_or("index out of range or no cacheable block")?;
    Ok(locations[resolved])
}

fn resolve_block_index(len: usize, index: Option<i64>) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match index {
        None => Some(len - 1),
        Some(0) => None,
        Some(index) if index > 0 => {
            let nth = usize::try_from(index).ok()?;
            (nth <= len).then(|| nth - 1)
        }
        Some(index) => {
            let from_end = usize::try_from(index.unsigned_abs()).ok()?;
            (from_end <= len).then(|| len - from_end)
        }
    }
}
