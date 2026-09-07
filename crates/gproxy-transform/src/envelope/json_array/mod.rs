use bytes::Bytes;
use serde_json::Value;

use super::SseFrame;
use crate::TransformError;

const MAX_BUFFER_BYTES: usize = 100 * 1024 * 1024;
const WIRE: &str = "Gemini JSON-array stream";

#[derive(Default)]
pub(super) struct JsonArrayDecoder {
    buffer: Vec<u8>,
    state: DecodeState,
}

#[derive(Default, PartialEq, Eq)]
enum DecodeState {
    #[default]
    Start,
    FirstOrEnd,
    Value,
    End,
}

impl JsonArrayDecoder {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, TransformError> {
        self.buffer.extend_from_slice(chunk);
        let frames = self.decode()?;
        if self.buffer.len() > MAX_BUFFER_BYTES {
            return Err(TransformError::shape(WIRE, "buffer exceeds 100 MiB"));
        }
        Ok(frames)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<SseFrame>, TransformError> {
        let frames = self.decode()?;
        if self.state == DecodeState::End {
            Ok(frames)
        } else {
            Err(TransformError::IncompleteStream)
        }
    }

    fn decode(&mut self) -> Result<Vec<SseFrame>, TransformError> {
        let mut frames = Vec::new();
        let mut cursor = 0;
        loop {
            cursor += whitespace_len(&self.buffer[cursor..]);
            match self.state {
                DecodeState::Start => match self.buffer.get(cursor) {
                    Some(b'[') => {
                        cursor += 1;
                        self.state = DecodeState::FirstOrEnd;
                    }
                    Some(_) => return Err(TransformError::shape(WIRE, "expected opening '['")),
                    None => break,
                },
                DecodeState::FirstOrEnd => match self.buffer.get(cursor) {
                    Some(b']') => {
                        cursor += 1;
                        self.state = DecodeState::End;
                    }
                    Some(_) => self.state = DecodeState::Value,
                    None => break,
                },
                DecodeState::Value => {
                    let Some((length, data)) = parse_value(&self.buffer[cursor..])? else {
                        break;
                    };
                    let end = cursor + length;
                    let separator = end + whitespace_len(&self.buffer[end..]);
                    let Some(byte) = self.buffer.get(separator).copied() else {
                        break;
                    };
                    self.state = match byte {
                        b',' => DecodeState::Value,
                        b']' => DecodeState::End,
                        _ => {
                            return Err(TransformError::shape(
                                WIRE,
                                "expected ',' or ']' after an element",
                            ));
                        }
                    };
                    cursor = separator + 1;
                    frames.push(SseFrame { event: None, data });
                }
                DecodeState::End => {
                    if cursor == self.buffer.len() {
                        break;
                    }
                    return Err(TransformError::shape(WIRE, "data after closing ']'"));
                }
            }
        }
        self.buffer.drain(..cursor);
        Ok(frames)
    }
}

fn whitespace_len(buffer: &[u8]) -> usize {
    buffer
        .iter()
        .position(|byte| !is_json_whitespace(*byte))
        .unwrap_or(buffer.len())
}

fn is_json_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t')
}

fn parse_value(buffer: &[u8]) -> Result<Option<(usize, String)>, TransformError> {
    let mut values = serde_json::Deserializer::from_slice(buffer).into_iter::<Value>();
    match values.next() {
        Some(Ok(value)) => {
            let end = values.byte_offset();
            if end > MAX_BUFFER_BYTES {
                return Err(TransformError::shape(WIRE, "element exceeds 100 MiB"));
            }
            Ok(Some((end, serde_json::to_string(&value)?)))
        }
        Some(Err(error)) if error.is_eof() => Ok(None),
        Some(Err(error)) => Err(TransformError::shape(
            WIRE,
            format!("invalid array element: {error}"),
        )),
        None => Ok(None),
    }
}

#[derive(Default)]
pub(super) struct JsonArrayEncoder {
    state: EncodeState,
}

#[derive(Default)]
enum EncodeState {
    #[default]
    Start,
    Streaming,
    End,
}

impl JsonArrayEncoder {
    pub(super) fn push(&mut self, data: &str) -> Result<Bytes, TransformError> {
        let data = data.trim_matches(|character| matches!(character, ' ' | '\n' | '\r' | '\t'));
        if data == "[DONE]" {
            return Err(TransformError::shape(
                WIRE,
                "[DONE] is not an array element",
            ));
        }
        if data.len() > MAX_BUFFER_BYTES {
            return Err(TransformError::shape(WIRE, "element exceeds 100 MiB"));
        }
        serde_json::from_str::<Value>(data)
            .map_err(|error| TransformError::shape(WIRE, format!("invalid element: {error}")))?;
        let prefix = match self.state {
            EncodeState::Start => b'[',
            EncodeState::Streaming => b',',
            EncodeState::End => return Err(TransformError::shape(WIRE, "element after finish")),
        };
        self.state = EncodeState::Streaming;
        let mut output = Vec::with_capacity(data.len() + 1);
        output.push(prefix);
        output.extend_from_slice(data.as_bytes());
        Ok(Bytes::from(output))
    }

    pub(super) fn finish(&mut self) -> Result<Bytes, TransformError> {
        let output = match self.state {
            EncodeState::Start => Bytes::from_static(b"[]"),
            EncodeState::Streaming => Bytes::from_static(b"]"),
            EncodeState::End => return Err(TransformError::shape(WIRE, "stream already finished")),
        };
        self.state = EncodeState::End;
        Ok(output)
    }
}

#[cfg(test)]
mod tests;
