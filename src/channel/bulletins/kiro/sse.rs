//! SSE emit + content helpers for the Kiro Responses decoder.
//!
//! Pure functions split out of [`response`](super::response) to keep that module
//! under the file-size budget: the `data:` framer, the two Responses output-item
//! builders, the delta-dedup + percent-decode content transforms, the Kiro→OpenAI
//! usage mapping, and the cross-target id generator. All ported from the v1
//! `gproxy-channel` kiro impl; all sync, so they compile on the wasm edge target.

use serde_json::{Value, json};

/// Append `data: {json}\n\n` to the SSE output buffer.
pub(super) fn push_sse(out: &mut Vec<u8>, value: Value) {
    out.extend_from_slice(b"data: ");
    match serde_json::to_vec(&value) {
        Ok(bytes) => out.extend(bytes),
        Err(_) => out.extend_from_slice(
            br#"{"type":"error","error":{"message":"serialize stream event failed"}}"#,
        ),
    }
    out.extend_from_slice(b"\n\n");
}

/// An assistant `message` output item.
pub(super) fn message_item(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id,
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [{ "type": "output_text", "text": text, "annotations": [] }],
    })
}

/// A `reasoning` output item.
pub(super) fn reasoning_item(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id,
        "type": "reasoning",
        "status": status,
        "summary": [],
        "content": [{ "type": "reasoning_text", "text": text }],
    })
}

/// Delta-dedup: Kiro re-sends accumulating prefixes; return only the NEW suffix
/// relative to `previous`, then store `chunk` as the new baseline. Identical or
/// shorter-prefix chunks yield "". Handles a partial overlap (longest suffix of
/// `previous` that prefixes `chunk`). Ported from v1 `normalize_kiro_chunk`.
pub(super) fn dedup_chunk(chunk: &str, previous: &mut String) -> String {
    if chunk.is_empty() {
        return String::new();
    }
    if previous.is_empty() {
        *previous = chunk.to_string();
        return chunk.to_string();
    }
    let prev = previous.as_bytes();
    let current = chunk.as_bytes();
    if current == prev {
        return String::new();
    }
    if current.starts_with(prev) {
        let delta = String::from_utf8_lossy(&current[prev.len()..]).into_owned();
        *previous = chunk.to_string();
        return delta;
    }
    if prev.starts_with(current) {
        return String::new();
    }
    let max_len = prev.len().min(current.len());
    let mut overlap = 0usize;
    for len in (1..=max_len).rev() {
        if prev.ends_with(&current[..len]) {
            overlap = len;
            break;
        }
    }
    *previous = chunk.to_string();
    if overlap > 0 {
        String::from_utf8_lossy(&current[overlap..]).into_owned()
    } else {
        chunk.to_string()
    }
}

/// Decode `%XX` percent-escapes to bytes (Kiro content is percent-encoded).
/// Non-escape bytes pass through; invalid UTF-8 falls back to the input.
pub(super) fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Map a Kiro `tokenUsage` object to OpenAI Responses `usage`. Tolerant of the
/// camelCase / snake_case key variants Kiro emits. Ported from v1.
pub(super) fn openai_usage(usage: Value) -> Option<Value> {
    let output = u64_any(
        &usage,
        &[
            "outputTokens",
            "completionTokens",
            "totalOutputTokens",
            "output_tokens",
            "completion_tokens",
            "total_output_tokens",
        ],
    )
    .unwrap_or(0);
    let cache_read =
        u64_any(&usage, &["cacheReadInputTokens", "cache_read_input_tokens"]).unwrap_or(0);
    let cache_write = u64_any(
        &usage,
        &[
            "cacheWriteInputTokens",
            "cacheCreationInputTokens",
            "cache_write_input_tokens",
            "cache_creation_input_tokens",
        ],
    )
    .unwrap_or(0);
    let uncached = u64_any(&usage, &["uncachedInputTokens", "uncached_input_tokens"]).unwrap_or(0);
    let input = u64_any(
        &usage,
        &[
            "inputTokens",
            "promptTokens",
            "totalInputTokens",
            "input_tokens",
            "prompt_tokens",
            "total_input_tokens",
        ],
    )
    .unwrap_or_else(|| {
        let cache_total = uncached + cache_read + cache_write;
        if cache_total > 0 {
            cache_total
        } else {
            u64_any(&usage, &["totalTokens", "total_tokens"])
                .and_then(|t| t.checked_sub(output))
                .unwrap_or(0)
        }
    });
    let total = u64_any(&usage, &["totalTokens", "total_tokens"]).unwrap_or(input + output);
    Some(json!({
        "input_tokens": input,
        "input_tokens_details": { "cached_tokens": cache_read },
        "output_tokens": output,
        "output_tokens_details": { "reasoning_tokens": 0 },
        "total_tokens": total,
    }))
}

fn u64_any(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| value.get(*k).and_then(json_u64))
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| {
            value
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| n as u64)
        })
        .or_else(|| value.as_str().and_then(|t| t.parse::<u64>().ok()))
}

/// Fresh `{prefix}_{hex}` id from `crate::util::rand` (one cross-target RNG
/// source; avoids uuid's native-only gate).
pub(super) fn gen_id(prefix: &str) -> String {
    let bytes = crate::util::rand::bytes::<16>();
    let mut out = String::with_capacity(prefix.len() + 1 + 32);
    out.push_str(prefix);
    out.push('_');
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    out
}
