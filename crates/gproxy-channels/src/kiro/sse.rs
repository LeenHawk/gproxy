use bytes::Bytes;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn frame(value: Value) -> gproxy_channel_api::Frame {
    let mut bytes = b"data: ".to_vec();
    match serde_json::to_vec(&value) {
        Ok(value) => bytes.extend(value),
        Err(_) => bytes.extend_from_slice(
            br#"{"type":"error","error":{"message":"serialize stream event failed"}}"#,
        ),
    }
    bytes.extend_from_slice(b"\n\n");
    gproxy_channel_api::Frame(Bytes::from(bytes))
}

pub(super) fn response(id: &str, model: &str, status: &str, output: Vec<Value>) -> Value {
    json!({
        "id":id,
        "object":"response",
        "model":model,
        "output":output,
        "status":status
    })
}

pub(super) fn message(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id":id,"type":"message","status":status,"role":"assistant",
        "content":[{"type":"output_text","text":text,"annotations":[]}]
    })
}

pub(super) fn reasoning(id: &str, text: &str, status: &str) -> Value {
    json!({
        "id":id,"type":"reasoning","status":status,"summary":[],
        "content":[{"type":"reasoning_text","text":text}]
    })
}

pub(super) fn id(prefix: &str, seed: &str) -> String {
    let digest = Sha256::digest(format!("kiro:{prefix}:{seed}"));
    let mut output = String::with_capacity(prefix.len() + 33);
    output.push_str(prefix);
    output.push('_');
    for byte in &digest[..16] {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

pub(super) fn dedup(value: &str, previous: &mut String) -> String {
    if value == previous || previous.starts_with(value) {
        return String::new();
    }
    if value.starts_with(previous.as_str()) {
        let delta = String::from_utf8_lossy(&value.as_bytes()[previous.len()..]).into_owned();
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
