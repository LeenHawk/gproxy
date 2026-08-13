//! Bounded settlement view of image SSE streams.
//!
//! Completed image events can carry a multi-megabyte `b64_json` string. The
//! normal SSE decoder deliberately caps frames at 1 MiB, so retain a second,
//! redacted transcript for usage extraction while the original bytes continue
//! to the client and the bounded downstream log buffer.

use bytes::Bytes;

use crate::pipeline::settle::{RelayBuffer, frames};

pub(super) struct ImageSseCapture {
    raw: RelayBuffer,
    settlement: RelayBuffer,
    redactor: B64Redactor,
}

impl ImageSseCapture {
    pub(super) fn new() -> Self {
        Self {
            raw: RelayBuffer::new(),
            settlement: RelayBuffer::new(),
            redactor: B64Redactor::default(),
        }
    }

    pub(super) fn push(&mut self, chunk: &Bytes) {
        self.raw.push(chunk.clone());
        let redacted = self.redactor.push(chunk);
        if !redacted.is_empty() {
            self.settlement.push(redacted);
        }
    }

    pub(super) fn completed(&self) -> bool {
        let body = self.settlement.concat();
        // A compatible upstream may ignore `stream: true` and return one
        // ordinary JSON image response through the streaming transport.
        if serde_json::from_slice::<serde_json::Value>(&body).is_ok() {
            return true;
        }
        frames::decode(&body)
            .ok()
            .into_iter()
            .flatten()
            .any(|frame| {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&frame.data) else {
                    return false;
                };
                matches!(
                    frame.event.as_deref(),
                    Some("image_generation.completed" | "image_edit.completed")
                ) || value
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|kind| {
                        matches!(kind, "image_generation.completed" | "image_edit.completed")
                            || kind == "transcript.text.done"
                    })
            })
    }

    pub(super) fn settlement_body(&self) -> Bytes {
        Bytes::from(self.settlement.concat())
    }

    pub(super) fn log_body(&self) -> Vec<u8> {
        self.raw.concat_for_log()
    }
}

const B64_KEY: &[u8] = b"b64_json";

#[derive(Default)]
struct B64Redactor {
    state: RedactState,
}

#[derive(Default)]
enum RedactState {
    #[default]
    Normal,
    String {
        matched: usize,
        possible_key: bool,
        escaped: bool,
    },
    AfterKey,
    AfterColon,
    Redacting {
        escaped: bool,
    },
}

impl B64Redactor {
    fn push(&mut self, chunk: &[u8]) -> Bytes {
        let mut output = Vec::with_capacity(chunk.len().min(8 * 1024));
        for &byte in chunk {
            let state = std::mem::take(&mut self.state);
            self.state = match state {
                RedactState::Normal => normal(byte, &mut output),
                RedactState::String {
                    mut matched,
                    mut possible_key,
                    mut escaped,
                } => {
                    output.push(byte);
                    if escaped {
                        escaped = false;
                        possible_key = false;
                        RedactState::String {
                            matched,
                            possible_key,
                            escaped,
                        }
                    } else if byte == b'\\' {
                        RedactState::String {
                            matched,
                            possible_key: false,
                            escaped: true,
                        }
                    } else if byte == b'"' {
                        if possible_key && matched == B64_KEY.len() {
                            RedactState::AfterKey
                        } else {
                            RedactState::Normal
                        }
                    } else {
                        possible_key &= B64_KEY.get(matched) == Some(&byte);
                        matched = matched.saturating_add(1);
                        RedactState::String {
                            matched,
                            possible_key,
                            escaped,
                        }
                    }
                }
                RedactState::AfterKey => {
                    output.push(byte);
                    if byte.is_ascii_whitespace() {
                        RedactState::AfterKey
                    } else if byte == b':' {
                        RedactState::AfterColon
                    } else {
                        normal_after_output(byte)
                    }
                }
                RedactState::AfterColon => {
                    output.push(byte);
                    if byte.is_ascii_whitespace() {
                        RedactState::AfterColon
                    } else if byte == b'"' {
                        RedactState::Redacting { escaped: false }
                    } else {
                        normal_after_output(byte)
                    }
                }
                RedactState::Redacting { mut escaped } => {
                    if escaped {
                        escaped = false;
                        RedactState::Redacting { escaped }
                    } else if byte == b'\\' {
                        RedactState::Redacting { escaped: true }
                    } else if byte == b'"' {
                        output.push(byte);
                        RedactState::Normal
                    } else {
                        RedactState::Redacting { escaped }
                    }
                }
            };
        }
        Bytes::from(output)
    }
}

fn normal(byte: u8, output: &mut Vec<u8>) -> RedactState {
    output.push(byte);
    normal_after_output(byte)
}

fn normal_after_output(byte: u8) -> RedactState {
    if byte == b'"' {
        RedactState::String {
            matched: 0,
            possible_key: true,
            escaped: false,
        }
    } else {
        RedactState::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_b64_values_across_chunk_boundaries_only() {
        let mut redactor = B64Redactor::default();
        let chunks = [
            br#"data: {"type":"image_generation.completed","b64_"#.as_slice(),
            br#"json" : "AAA"#.as_slice(),
            br#"A\\\"BBBB","note":"b64_json","usage":{"input_tokens":1,"output_tokens":2}}\n\n"#
                .as_slice(),
        ];
        let redacted = chunks
            .into_iter()
            .flat_map(|chunk| redactor.push(chunk).to_vec())
            .collect::<Vec<_>>();
        let text = String::from_utf8(redacted).unwrap();
        assert!(text.contains(r#""b64_json" : """#));
        assert!(text.contains(r#""note":"b64_json""#));
        assert!(text.contains(r#""usage":{"input_tokens":1,"output_tokens":2}"#));
        assert!(!text.contains("AAAA"));
    }

    #[test]
    fn large_completed_frame_stays_decodable_for_settlement() {
        let completed = serde_json::json!({
            "type": "image_generation.completed",
            "b64_json": "A".repeat((1024 * 1024) + 1),
            "usage": {
                "prompt_tokens": 25,
                "completion_tokens": 1000,
                "total_tokens": 1025
            }
        });
        let wire = Bytes::from(format!("data: {completed}\n\n"));
        let mut capture = ImageSseCapture::new();
        for chunk in wire.chunks(8191) {
            capture.push(&Bytes::copy_from_slice(chunk));
        }

        assert!(capture.completed());
        assert!(capture.settlement_body().len() < 1024);
        let frames = frames::decode(&capture.settlement_body()).unwrap();
        let usage = crate::usage::extract::from_image_stream_frames(
            crate::protocol::Provider::OpenAi,
            &frames,
        )
        .unwrap();
        assert_eq!(usage.input, 25);
        assert_eq!(usage.image_output, 1000);

        let partial = Bytes::from_static(
            b"data: {\"type\":\"image_generation.partial_image\",\"b64_json\":\"AA\"}\n\n",
        );
        let mut capture = ImageSseCapture::new();
        capture.push(&partial);
        assert!(!capture.completed());

        let mut capture = ImageSseCapture::new();
        capture.push(&Bytes::from_static(
            b"event: image_generation.completed\ndata: {\"type\":\"image_generation.completed\",\"b64_json\":\"AA",
        ));
        assert!(
            !capture.completed(),
            "a truncated completed event is not EOF-complete"
        );

        let mut capture = ImageSseCapture::new();
        capture.push(&Bytes::from_static(
            b"{\"created\":1,\"data\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2}}",
        ));
        assert!(capture.completed());
    }
}
