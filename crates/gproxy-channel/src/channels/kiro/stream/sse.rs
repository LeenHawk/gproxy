//! The Responses events the translator synthesizes, and the text repair the
//! upstream's deltas need (v3 `kiro/sse.rs`).

use gproxy_protocol::connection::Bytes;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// One SSE record. The Responses stream names its event only in the payload's
/// `type`, so no `event:` line is written (v3 `sse::frame`).
pub(super) fn frame(value: &Value) -> Bytes {
    Bytes::from(format!("data: {value}\n\n"))
}

pub(super) fn response(id: &str, model: &str, status: &str, output: Vec<Value>) -> Value {
    json!({
        "id": id,
        "object": "response",
        "model": model,
        "output": output,
        "status": status,
    })
}

pub(super) fn message(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id, "type": "message", "status": status, "role": "assistant",
        "content": [{"type": "output_text", "text": text, "annotations": []}],
    })
}

pub(super) fn reasoning(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id": id, "type": "reasoning", "status": status, "summary": [],
        "content": [{"type": "reasoning_text", "text": text}],
    })
}

/// A stable Responses item id derived from the conversation, so a reconnect
/// that replays the same conversation names the same items.
pub(super) fn id(prefix: &str, seed: &str) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(format!("kiro:{prefix}:{seed}"));
    let mut output = String::with_capacity(prefix.len() + 33);
    output.push_str(prefix);
    output.push('_');
    for byte in &digest[..16] {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

/// The upstream repeats text: a frame may resend what the previous one said,
/// or overlap with it. Only the part that is new becomes a delta (v3
/// `sse::dedup`).
pub(super) fn dedup(value: &str, previous: &mut String) -> String {
    if value == previous || previous.starts_with(value) {
        return String::new();
    }
    if value.starts_with(previous.as_str()) {
        let delta = value[previous.len()..].to_owned();
        *previous = value.into();
        return delta;
    }
    let old = previous.as_bytes();
    let new = value.as_bytes();
    let overlap = (1..=old.len().min(new.len()))
        .rev()
        .find(|length| old.ends_with(&new[..*length]))
        .unwrap_or_default();
    *previous = value.into();
    String::from_utf8_lossy(&new[overlap..]).into_owned()
}

/// Some upstream builds percent-encode the assistant's text. A sequence that
/// is not valid encoding is left exactly as it arrived (v3 `sse::percent_decode`).
pub(super) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).unwrap_or_else(|_| value.into())
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_is_new_becomes_a_delta() {
        let mut previous = String::new();
        assert_eq!(dedup("Hel", &mut previous), "Hel");
        assert_eq!(dedup("Hello", &mut previous), "lo");
        assert_eq!(dedup("Hello", &mut previous), "", "a repeat is not a delta");
        assert_eq!(dedup("Hel", &mut previous), "", "nor is a prefix of it");
        assert_eq!(dedup("llo world", &mut previous), " world", "overlap");
    }

    #[test]
    fn percent_sequences_decode_and_the_rest_survives() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100% sure"), "100% sure");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%E4%B8%AD"), "中");
    }
}
