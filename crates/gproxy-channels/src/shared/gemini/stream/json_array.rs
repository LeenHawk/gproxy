//! Incremental JSON-array framing used by Gemini streaming responses.

use gproxy_channel_api::ChannelError;
use gproxy_protocol::gemini::GenerateContentResponse;

const MAX_BUFFER_BYTES: usize = 100 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Decoder {
    buffer: Vec<u8>,
    state: State,
}

#[derive(Default, PartialEq, Eq)]
enum State {
    #[default]
    Start,
    FirstOrEnd,
    Value,
    End,
}

impl Decoder {
    pub(super) fn push(
        &mut self,
        chunk: &[u8],
    ) -> Result<Vec<GenerateContentResponse>, ChannelError> {
        self.buffer.extend_from_slice(chunk);
        let output = self.decode()?;
        if self.buffer.len() > MAX_BUFFER_BYTES {
            return Err(decode("buffer exceeds 100 MiB"));
        }
        Ok(output)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<GenerateContentResponse>, ChannelError> {
        let output = self.decode()?;
        if self.state == State::End {
            Ok(output)
        } else {
            Err(decode("stream ended before closing ']'"))
        }
    }

    fn decode(&mut self) -> Result<Vec<GenerateContentResponse>, ChannelError> {
        let mut output = Vec::new();
        let mut cursor = 0;
        loop {
            cursor += whitespace_len(&self.buffer[cursor..]);
            match self.state {
                State::Start => match self.buffer.get(cursor) {
                    Some(b'[') => {
                        cursor += 1;
                        self.state = State::FirstOrEnd;
                    }
                    Some(_) => return Err(decode("expected opening '['")),
                    None => break,
                },
                State::FirstOrEnd => match self.buffer.get(cursor) {
                    Some(b']') => {
                        cursor += 1;
                        self.state = State::End;
                    }
                    Some(_) => self.state = State::Value,
                    None => break,
                },
                State::Value => {
                    let Some((length, value)) = parse_value(&self.buffer[cursor..])? else {
                        break;
                    };
                    let end = cursor + length;
                    let separator = end + whitespace_len(&self.buffer[end..]);
                    let Some(byte) = self.buffer.get(separator).copied() else {
                        break;
                    };
                    self.state = match byte {
                        b',' => State::Value,
                        b']' => State::End,
                        _ => return Err(decode("expected ',' or ']' after an element")),
                    };
                    cursor = separator + 1;
                    output.push(value);
                }
                State::End => {
                    if cursor == self.buffer.len() {
                        break;
                    }
                    return Err(decode("data followed closing ']'"));
                }
            }
        }
        self.buffer.drain(..cursor);
        Ok(output)
    }
}

fn parse_value(buffer: &[u8]) -> Result<Option<(usize, GenerateContentResponse)>, ChannelError> {
    let mut values =
        serde_json::Deserializer::from_slice(buffer).into_iter::<GenerateContentResponse>();
    match values.next() {
        Some(Ok(value)) => {
            let end = values.byte_offset();
            if end > MAX_BUFFER_BYTES {
                return Err(decode("element exceeds 100 MiB"));
            }
            Ok(Some((end, value)))
        }
        Some(Err(error)) if error.is_eof() => Ok(None),
        Some(Err(error)) => Err(decode(&format!("invalid array element: {error}"))),
        None => Ok(None),
    }
}

fn whitespace_len(buffer: &[u8]) -> usize {
    buffer
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        .unwrap_or(buffer.len())
}

fn decode(message: &str) -> ChannelError {
    ChannelError::Decode(format!("Gemini JSON-array stream: {message}"))
}
