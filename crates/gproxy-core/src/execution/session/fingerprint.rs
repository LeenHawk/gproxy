use gproxy_protocol::{ContentGenerationKind, OperationKind};
use serde_json::Value;
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"gproxy:session-affinity:conversation:v1";

#[derive(Clone, Copy)]
struct Segment<'a> {
    role: &'static [u8],
    source: &'static [u8],
    content: &'a Value,
}

pub(super) fn digest(kind: OperationKind, body: &Value) -> Option<[u8; 32]> {
    let (family, segments) = match kind {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
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
        OperationKind::Family(_) => return None,
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
    Some(encoder.finish())
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

fn non_null(value: Option<&Value>) -> bool {
    value.is_some_and(|value| !value.is_null())
}

struct CanonicalEncoder(Sha256);

impl CanonicalEncoder {
    fn new() -> Self {
        Self(Sha256::new())
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }

    fn header(&mut self, tag: u8, len: usize) {
        self.0.update([tag]);
        self.0.update((len as u64).to_be_bytes());
    }

    fn bytes(&mut self, tag: u8, bytes: &[u8]) {
        self.header(tag, bytes.len());
        self.0.update(bytes);
    }

    fn value(&mut self, value: &Value) {
        match value {
            Value::Null => self.header(b'n', 0),
            Value::Bool(value) => {
                self.header(b'b', 1);
                self.0.update([u8::from(*value)]);
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
    use gproxy_protocol::ContentGenerationKind::*;
    use serde_json::json;

    use super::*;

    #[test]
    fn supported_wires_ignore_appended_turns() {
        let cases = [
            (
                OpenAiChat,
                json!({"messages":[
                    {"role":"system","content":"policy"},
                    {"role":"user","content":"question"}
                ]}),
                json!({"messages":[
                    {"role":"system","content":"policy"},
                    {"role":"user","content":"question"},
                    {"role":"assistant","content":"answer"}
                ]}),
            ),
            (
                ClaudeMessages,
                json!({"system":"policy","messages":[
                    {"role":"user","content":"question"}
                ]}),
                json!({"system":"policy","messages":[
                    {"role":"user","content":"question"},
                    {"role":"assistant","content":"answer"}
                ]}),
            ),
            (
                GeminiGenerateContent,
                json!({"systemInstruction":{"parts":[{"text":"policy"}]},"contents":[
                    {"role":"user","parts":[{"text":"question"}]}
                ]}),
                json!({"systemInstruction":{"parts":[{"text":"policy"}]},"contents":[
                    {"role":"user","parts":[{"text":"question"}]},
                    {"role":"model","parts":[{"text":"answer"}]}
                ]}),
            ),
            (
                OpenAiResponses,
                json!({"instructions":"policy","input":[
                    {"role":"user","content":"question"}
                ]}),
                json!({"instructions":"policy","input":[
                    {"role":"user","content":"question"},
                    {"role":"assistant","content":"answer"}
                ]}),
            ),
        ];
        for (kind, head, appended) in cases {
            let kind = OperationKind::ContentGeneration(kind);
            assert_eq!(digest(kind, &head), digest(kind, &appended));
            assert!(digest(kind, &head).is_some());
        }
    }

    #[test]
    fn native_server_state_refuses_a_guessed_fingerprint() {
        assert!(
            digest(
                OperationKind::ContentGeneration(OpenAiResponses),
                &json!({"previous_response_id":"resp_1","input":"next"}),
            )
            .is_none()
        );
        assert!(
            digest(
                OperationKind::ContentGeneration(ClaudeMessages),
                &json!({
                    "diagnostics":{"previous_message_id":"msg_1"},
                    "messages":[{"role":"user","content":"next"}]
                }),
            )
            .is_none()
        );
        assert!(
            digest(
                OperationKind::ContentGeneration(GeminiGenerateContent),
                &json!({"cachedContent":"cached/1","contents":[]}),
            )
            .is_none()
        );
    }
}
