use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody,
    connection::{ByteStream, Bytes},
};
use serde_json::{Value, json};

use super::{request::invalid, wire};
use gproxy_upstream::UpstreamError;

/// JSPB's RPC status envelope can omit the first slot: `[,[7,"..."]]`.
/// Normalize only omitted array entries, never the contents of JSON strings.
pub(super) fn parse_jspb(bytes: &[u8]) -> Result<Value, UpstreamError> {
    let mut out = Vec::with_capacity(bytes.len());
    let (mut string, mut escape, mut previous) = (false, false, 0);
    for &byte in bytes {
        if string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            if byte == b',' && matches!(previous, b'[' | b',') {
                out.extend_from_slice(b"null");
            }
            if byte == b'"' {
                string = true;
            }
        }
        out.push(byte);
        if !byte.is_ascii_whitespace() {
            previous = byte;
        }
    }
    serde_json::from_slice(&out).map_err(|_| invalid("invalid JSPB JSON"))
}

/// The RPC stream is `[ [response, response, ...], optional_status ]`.
/// Yield each response as its array closes, without buffering the entire
/// outer RPC envelope. A status trailer or truncated envelope is an error.
#[derive(Default)]
struct Decoder {
    depth: usize,
    outer_field: usize,
    value: Vec<u8>,
    string: bool,
    escape: bool,
    started: bool,
    closed: bool,
    trailer: bool,
    finished: bool,
}

impl Decoder {
    fn push(&mut self, bytes: &[u8], model: &str) -> Result<Vec<Value>, UpstreamError> {
        let mut values = vec![];
        for &byte in bytes {
            if self.closed {
                if byte.is_ascii_whitespace() {
                    continue;
                }
                return Err(invalid("data after RPC stream end"));
            }
            if self.string {
                self.value.push(byte);
                if self.escape {
                    self.escape = false;
                } else if byte == b'\\' {
                    self.escape = true;
                } else if byte == b'"' {
                    self.string = false;
                }
                continue;
            }
            if byte == b'[' {
                if self.depth == 0 {
                    self.started = true;
                }
                self.depth += 1;
                if self.depth == 2 && self.outer_field > 0 {
                    self.trailer = true;
                }
                if self.depth >= 3 || self.trailer {
                    self.value.push(byte);
                }
            } else if byte == b']' {
                if self.depth == 0 {
                    return Err(invalid("unmatched RPC array end"));
                }
                if self.depth >= 3 || self.trailer {
                    self.value.push(byte);
                }
                if self.depth == 3 && !self.trailer {
                    let raw = parse_jspb(&std::mem::take(&mut self.value))?;
                    if let Some(response) = wire::response(&raw, model)? {
                        self.finished |= response
                            .get("candidates")
                            .and_then(Value::as_array)
                            .is_some_and(|items| {
                                items.iter().any(|v| v.get("finishReason").is_some())
                            });
                        values.push(response);
                    }
                } else if self.depth == 2 && self.trailer {
                    let status = parse_jspb(&std::mem::take(&mut self.value))?;
                    if status
                        .get(0)
                        .and_then(Value::as_u64)
                        .is_some_and(|code| code != 0)
                    {
                        return Err(invalid(
                            status
                                .get(1)
                                .and_then(Value::as_str)
                                .unwrap_or("upstream RPC stream failed"),
                        ));
                    }
                    self.trailer = false;
                }
                self.depth -= 1;
                if self.depth == 0 {
                    self.closed = true;
                }
            } else if self.depth >= 3 || self.trailer {
                self.value.push(byte);
                if byte == b'"' {
                    self.string = true;
                }
            } else if byte == b',' && self.depth == 1 {
                self.outer_field += 1;
            } else if !byte.is_ascii_whitespace() && byte != b',' {
                return Err(invalid("unexpected RPC envelope value"));
            }
        }
        Ok(values)
    }

    fn finish(&self) -> Result<(), UpstreamError> {
        if !self.started || !self.closed || self.depth != 0 || self.string || !self.value.is_empty()
        {
            return Err(invalid("truncated RPC stream"));
        }
        if !self.finished {
            return Err(invalid("RPC stream ended without a finish reason"));
        }
        Ok(())
    }
}

fn bytes(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
    }
}

pub(super) fn translate(body: HttpBody, model: String) -> ByteStream {
    Box::pin(futures_util::stream::unfold(
        Some((bytes(body), Decoder::default(), model)),
        |state| async move {
            let (mut upstream, mut decoder, model) = state?;
            loop {
                match upstream.next().await {
                    Some(Ok(chunk)) => match decoder.push(&chunk, &model) {
                        Ok(values) if values.is_empty() => continue,
                        Ok(values) => {
                            let output = values
                                .iter()
                                .map(|value| format!("data: {value}\n\n"))
                                .collect::<String>();
                            return Some((
                                Ok(Bytes::from(output)),
                                Some((upstream, decoder, model)),
                            ));
                        }
                        Err(error) => return Some((Err(error.to_string().into()), None)),
                    },
                    Some(Err(error)) => return Some((Err(error), None)),
                    None => {
                        return decoder
                            .finish()
                            .err()
                            .map(|error| (Err(error.to_string().into()), None));
                    }
                }
            }
        },
    ))
}

pub(super) async fn collect(body: HttpBody, model: &str) -> Result<Value, UpstreamError> {
    let mut upstream = bytes(body);
    let mut decoder = Decoder::default();
    let mut output = json!({"candidates":[],"modelVersion":model.trim_start_matches("models/")});
    while let Some(chunk) = upstream.next().await {
        for value in decoder.push(&chunk.map_err(|error| invalid(error.to_string()))?, model)? {
            for key in ["usageMetadata", "responseId"] {
                if let Some(value) = value.get(key) {
                    output[key] = value.clone();
                }
            }
            for candidate in value
                .get("candidates")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let index = candidate["index"].as_u64().unwrap_or(0) as usize;
                let candidates = output["candidates"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("invalid candidate accumulator"))?;
                while candidates.len() <= index {
                    candidates.push(
                        json!({"index":candidates.len(),"content":{"role":"model","parts":[]}}),
                    );
                }
                let current = &mut candidates[index];
                if let Some(reason) = candidate.get("finishReason") {
                    current["finishReason"] = reason.clone();
                }
                if let Some(parts) = candidate
                    .pointer("/content/parts")
                    .and_then(Value::as_array)
                {
                    current["content"]["parts"]
                        .as_array_mut()
                        .ok_or_else(|| invalid("invalid parts accumulator"))?
                        .extend(parts.iter().cloned());
                }
            }
        }
    }
    decoder.finish()?;
    Ok(output)
}
