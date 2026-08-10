//! Stable conversation-head fingerprints for route affinity.
//!
//! This is deliberately a narrow recognizer, not a permissive protocol parser.
//! Unsupported or ambiguous prefixes return `None`, so affinity falls back to
//! the user key rather than risking a false conversation binding.

use serde_json::Value;

use crate::protocol::{ContentGenerationKind, OperationKey, OperationKind};

const DOMAIN: &[u8] = b"gproxy:route-affinity:conversation:v1";

#[derive(Clone, Copy)]
struct Segment<'a> {
    role: &'static [u8],
    source: &'static [u8],
    content: &'a Value,
}

pub(super) fn fingerprint(op: OperationKey, body: &Value) -> Option<[u8; 32]> {
    let (family, segments) = match op.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChatCompletions) => {
            (b"chat".as_slice(), chat_segments(body)?)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            (b"claude".as_slice(), claude_segments(body)?)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            (b"gemini".as_slice(), gemini_segments(body)?)
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => (b"responses".as_slice(), responses_segments(body)?),
        _ => return None,
    };

    let mut encoder = CanonicalEncoder::new();
    encoder.bytes(b'D', DOMAIN);
    encoder.bytes(b'F', family);
    encoder.header(b'L', segments.len());
    for segment in segments {
        encoder.header(b'G', 3);
        encoder.bytes(b'R', segment.role);
        encoder.bytes(b'S', segment.source);
        encoder.value(segment.content);
    }
    Some(*encoder.finish().as_bytes())
}

fn chat_segments(body: &Value) -> Option<Vec<Segment<'_>>> {
    prefixed_messages(body.get("messages")?, b"messages")
}

fn claude_segments(body: &Value) -> Option<Vec<Segment<'_>>> {
    if non_null(body.get("container"))
        || non_null(
            body.get("diagnostics")
                .and_then(|diagnostics| diagnostics.get("previous_message_id")),
        )
    {
        return None;
    }

    let mut segments = Vec::new();
    push_system(
        &mut segments,
        b"system",
        b"system",
        system_content(body.get("system"))?,
    );

    let first = body.get("messages")?.as_array()?.first()?.as_object()?;
    if first.get("role")?.as_str()? != "user" {
        return None;
    }
    segments.push(Segment {
        role: b"user",
        source: b"messages",
        content: user_content(first.get("content"))?,
    });
    Some(segments)
}

fn gemini_segments(body: &Value) -> Option<Vec<Segment<'_>>> {
    if non_null(body.get("cachedContent")) {
        return None;
    }

    let mut segments = Vec::new();
    match body.get("systemInstruction") {
        None | Some(Value::Null) => {}
        Some(Value::Object(instruction)) => push_system(
            &mut segments,
            b"system",
            b"systemInstruction",
            system_content(instruction.get("parts"))?,
        ),
        Some(_) => return None,
    }

    for content in body.get("contents")?.as_array()? {
        let content = content.as_object()?;
        match content.get("role") {
            Some(Value::String(role)) if role == "system" => push_system(
                &mut segments,
                b"system",
                b"contents",
                system_content(content.get("parts"))?,
            ),
            None | Some(Value::Null) => {
                segments.push(Segment {
                    role: b"user",
                    source: b"contents",
                    content: user_content(content.get("parts"))?,
                });
                return Some(segments);
            }
            Some(Value::String(role)) if role == "user" => {
                segments.push(Segment {
                    role: b"user",
                    source: b"contents",
                    content: user_content(content.get("parts"))?,
                });
                return Some(segments);
            }
            _ => return None,
        }
    }
    None
}

fn responses_segments(body: &Value) -> Option<Vec<Segment<'_>>> {
    if non_null(body.get("previous_response_id")) || non_null(body.get("conversation")) {
        return None;
    }

    let mut segments = Vec::new();
    push_system(
        &mut segments,
        b"developer",
        b"instructions",
        system_content(body.get("instructions"))?,
    );
    match body.get("input")? {
        value @ Value::String(text) if !text.is_empty() => {
            segments.push(Segment {
                role: b"user",
                source: b"input",
                content: value,
            });
            Some(segments)
        }
        Value::Array(messages) => prefixed_message_array(messages, b"input", segments),
        _ => None,
    }
}

fn non_null(value: Option<&Value>) -> bool {
    value.is_some_and(|value| !value.is_null())
}

fn prefixed_messages<'a>(value: &'a Value, source: &'static [u8]) -> Option<Vec<Segment<'a>>> {
    prefixed_message_array(value.as_array()?, source, Vec::new())
}

fn prefixed_message_array<'a>(
    messages: &'a [Value],
    source: &'static [u8],
    mut segments: Vec<Segment<'a>>,
) -> Option<Vec<Segment<'a>>> {
    for message in messages {
        let message = message.as_object()?;
        match message.get("role")?.as_str()? {
            "system" => push_system(
                &mut segments,
                b"system",
                source,
                system_content(message.get("content"))?,
            ),
            "developer" => push_system(
                &mut segments,
                b"developer",
                source,
                system_content(message.get("content"))?,
            ),
            "user" => {
                segments.push(Segment {
                    role: b"user",
                    source,
                    content: user_content(message.get("content"))?,
                });
                return Some(segments);
            }
            _ => return None,
        }
    }
    None
}

fn push_system<'a>(
    segments: &mut Vec<Segment<'a>>,
    role: &'static [u8],
    source: &'static [u8],
    content: Option<&'a Value>,
) {
    if let Some(content) = content {
        segments.push(Segment {
            role,
            source,
            content,
        });
    }
}

/// System/developer content may be omitted or empty. Any non-empty shape that
/// cannot be walked deterministically rejects the whole fingerprint.
fn system_content(value: Option<&Value>) -> Option<Option<&Value>> {
    match value {
        None | Some(Value::Null) => Some(None),
        Some(value @ Value::String(text)) => Some((!text.is_empty()).then_some(value)),
        Some(value @ Value::Array(parts)) => parts
            .iter()
            .all(Value::is_object)
            .then(|| (!parts.is_empty()).then_some(value)),
        Some(_) => None,
    }
}

fn user_content(value: Option<&Value>) -> Option<&Value> {
    match value? {
        value @ Value::String(text) if !text.is_empty() => Some(value),
        value @ Value::Array(parts) if !parts.is_empty() && parts.iter().all(Value::is_object) => {
            Some(value)
        }
        _ => None,
    }
}

/// Version-locked canonical encoder. Every semantic field and JSON value is a
/// one-byte type tag followed by a big-endian u64 length/count. JSON object keys
/// are sorted by raw UTF-8 bytes; strings are otherwise preserved exactly.
struct CanonicalEncoder {
    hasher: blake3::Hasher,
}

impl CanonicalEncoder {
    fn new() -> Self {
        Self {
            hasher: blake3::Hasher::new(),
        }
    }

    fn finish(self) -> blake3::Hash {
        self.hasher.finalize()
    }

    fn header(&mut self, tag: u8, len: usize) {
        self.hasher.update(&[tag]);
        self.hasher.update(&(len as u64).to_be_bytes());
    }

    fn bytes(&mut self, tag: u8, bytes: &[u8]) {
        self.header(tag, bytes.len());
        self.hasher.update(bytes);
    }

    fn value(&mut self, value: &Value) {
        match value {
            Value::Null => self.header(b'n', 0),
            Value::Bool(value) => {
                self.header(b'b', 1);
                self.hasher.update(&[u8::from(*value)]);
            }
            Value::Number(value) => self.bytes(b'd', value.to_string().as_bytes()),
            Value::String(value) => self.bytes(b's', value.as_bytes()),
            Value::Array(values) => {
                self.header(b'a', values.len());
                for value in values {
                    self.value(value);
                }
            }
            Value::Object(values) => {
                self.header(b'o', values.len());
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
                for key in keys {
                    self.bytes(b'k', key.as_bytes());
                    self.value(&values[key]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::protocol::{ContentGenerationKind as Kind, Operation, OperationKey};

    fn op(kind: Kind) -> OperationKey {
        OperationKey::content_generation(Operation::GenerateContent, kind)
    }

    fn digest(kind: Kind, body: Value) -> Option<[u8; 32]> {
        fingerprint(op(kind), &body)
    }

    fn hex(digest: [u8; 32]) -> String {
        blake3::Hash::from_bytes(digest).to_hex().to_string()
    }

    #[test]
    fn chat_fingerprint_has_a_fixed_golden_value() {
        let body = json!({
            "model": "ignored",
            "stream": true,
            "messages": [
                {"role": "system", "content": "system"},
                {"role": "developer", "content": [{
                    "type": "text",
                    "text": "policy",
                    "extra": {"b": 2, "a": 1},
                    "flags": [true, false, null, 3.25]
                }]},
                {"role": "user", "content": [
                    {"type": "text", "text": " hello "},
                    {"type": "image_url", "image_url": {
                        "url": "https://example.test/image.png", "detail": "low"
                    }}
                ]},
                {"role": "assistant", "content": 7}
            ],
            "tools": [{"type": "function", "function": {"name": "ignored"}}]
        });
        assert_eq!(
            hex(digest(Kind::OpenAiChatCompletions, body).unwrap()),
            "24abf9a398d3007ae4561fc2c2c5abf604c62bfdbe4da121dc53b9647484d833"
        );
    }

    #[test]
    fn ignored_fields_and_tail_do_not_change_chat_fingerprint() {
        let left: Value = serde_json::from_str(
            r#"{
          "model":"one","stream":false,"temperature":0.1,
          "messages":[
            {"role":"developer","name":"left","content":[{"z":2,"a":1}]},
            {"role":"user","id":"left","content":"question"},
            {"role":"assistant","content":{"unsupported":"tail"}}
          ]
        }"#,
        )
        .unwrap();
        let right: Value = serde_json::from_str(
            r#"{
          "model":"two","stream":true,"tools":[1],
          "messages":[
            {"type":"message","content":[{"a":1,"z":2}],"role":"developer"},
            {"status":"completed","content":"question","role":"user"},
            42
          ]
        }"#,
        )
        .unwrap();
        assert_eq!(
            digest(Kind::OpenAiChatCompletions, left),
            digest(Kind::OpenAiChatCompletions, right)
        );

        let omitted = json!({"messages": [{"role": "user", "content": "question"}]});
        let empty = json!({"messages": [
            {"role": "system", "content": ""},
            {"role": "user", "content": "question"}
        ]});
        assert_eq!(
            digest(Kind::OpenAiChatCompletions, omitted),
            digest(Kind::OpenAiChatCompletions, empty)
        );
    }

    #[test]
    fn all_supported_families_extract_the_conversation_head() {
        let claude = json!({
            "system": [{"type": "text", "text": "policy"}],
            "messages": [
                {"role": "user", "content": "question"},
                {"role": "assistant", "content": 1}
            ]
        });
        let gemini = json!({
            "systemInstruction": {"parts": [{"text": "policy"}]},
            "contents": [
                {"role": "system", "parts": [{"text": "extra"}]},
                {"parts": [{"text": "question"}]},
                {"role": false, "parts": []}
            ]
        });
        let responses = json!({
            "instructions": "policy",
            "input": [
                {"role": "system", "content": [{"type": "input_text", "text": "extra"}]},
                {"role": "developer", "content": "more"},
                {"role": "user", "content": "question"},
                {"role": false, "content": []}
            ]
        });
        assert!(digest(Kind::ClaudeMessages, claude).is_some());
        assert!(digest(Kind::GeminiGenerateContent, gemini).is_some());
        assert!(digest(Kind::OpenAiResponses, responses.clone()).is_some());
        assert_eq!(
            digest(Kind::OpenAiResponses, responses.clone()),
            digest(Kind::OpenAiResponsesWebSocket, responses)
        );
        let direct = json!({"instructions": "policy", "input": "question"});
        let message = json!({
            "instructions": "policy",
            "input": [{
                "type": "message", "id": "ignored", "status": "completed",
                "role": "user", "content": "question"
            }]
        });
        assert_eq!(
            digest(Kind::OpenAiResponses, direct),
            digest(Kind::OpenAiResponses, message)
        );
    }

    #[test]
    fn ambiguous_or_unsupported_prefixes_are_rejected() {
        let invalid = [
            (
                Kind::OpenAiChatCompletions,
                json!({"messages": [
                    {"role": "assistant", "content": "answer"},
                    {"role": "user", "content": "question"}
                ]}),
            ),
            (
                Kind::OpenAiChatCompletions,
                json!({"messages": [
                    {"role": "system", "content": 1},
                    {"role": "user", "content": "question"}
                ]}),
            ),
            (
                Kind::OpenAiChatCompletions,
                json!({"messages": [
                    {"role": "user", "content": ""}
                ]}),
            ),
            (
                Kind::ClaudeMessages,
                json!({"messages": [
                    {"role": "assistant", "content": "answer"}
                ]}),
            ),
            (
                Kind::ClaudeMessages,
                json!({
                    "container": "container_1",
                    "messages": [{"role": "user", "content": "question"}]
                }),
            ),
            (
                Kind::ClaudeMessages,
                json!({
                    "diagnostics": {"previous_message_id": "msg_1"},
                    "messages": [{"role": "user", "content": "question"}]
                }),
            ),
            (
                Kind::GeminiGenerateContent,
                json!({"contents": [
                    {"role": "model", "parts": [{"text": "answer"}]}
                ]}),
            ),
            (
                Kind::GeminiGenerateContent,
                json!({
                    "cachedContent": "cachedContents/example",
                    "contents": [{"role": "user", "parts": [{"text": "question"}]}]
                }),
            ),
            (
                Kind::GeminiGenerateContent,
                json!({"contents": [
                    {"role": "user", "parts": [{"text": "ok"}, 1]}
                ]}),
            ),
            (
                Kind::OpenAiResponses,
                json!({
                    "previous_response_id": "resp_1", "input": "question"
                }),
            ),
            (
                Kind::OpenAiResponses,
                json!({
                    "conversation": {}, "input": "question"
                }),
            ),
        ];
        for (kind, body) in invalid {
            assert_eq!(digest(kind, body), None);
        }
    }
}
