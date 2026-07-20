use serde_json::{Map, Value, json};

pub(super) fn resolve_location<T: Copy>(
    locations: &[T],
    index: Option<i64>,
) -> Result<T, &'static str> {
    let index =
        resolve_block_index(locations.len(), index).ok_or("index out of range or invalid")?;
    Ok(locations[index])
}

fn resolve_block_index(len: usize, index: Option<i64>) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match index {
        None => Some(len - 1),
        Some(0) => None,
        Some(i) if i > 0 => {
            let nth = i as usize;
            (nth <= len).then(|| nth - 1)
        }
        Some(i) => {
            let from_end = i.unsigned_abs() as usize;
            (from_end <= len).then(|| len - from_end)
        }
    }
}

pub(super) fn set_prompt_cache_options(
    root: &mut Map<String, Value>,
    mode: Option<&str>,
    ttl: Option<&str>,
) -> Result<(), &'static str> {
    let options = root
        .entry("prompt_cache_options")
        .or_insert_with(|| json!({}));
    let options = options
        .as_object_mut()
        .ok_or("prompt_cache_options is not an object")?;
    if let Some(mode) = mode {
        options.entry("mode").or_insert_with(|| json!(mode));
    }
    if ttl == Some("30m") {
        options.insert("ttl".into(), json!("30m"));
    }
    Ok(())
}
