use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use serde::Serialize;

use gproxy_protocol::gemini;
use gproxy_protocol::claude::create_message::stream::{BetaStreamEvent, BetaStreamEventKnown};
use gproxy_protocol::sse::SseParser;

pub(super) fn gemini_stream_to_generate(
    request: gemini::stream_content::request::StreamGenerateContentRequest,
) -> gemini::generate_content::request::GenerateContentRequest {
    gemini::generate_content::request::GenerateContentRequest {
        path: request.path,
        body: request.body,
    }
}

pub(super) fn gemini_generate_to_stream(
    request: gemini::generate_content::request::GenerateContentRequest,
) -> gemini::stream_content::request::StreamGenerateContentRequest {
    gemini::stream_content::request::StreamGenerateContentRequest {
        path: request.path,
        body: request.body,
    }
}

pub(super) fn sse_json_bytes<T: Serialize>(value: &T) -> Option<Bytes> {
    let payload = serde_json::to_vec(value).ok()?;
    let mut data = Vec::with_capacity(payload.len() + 8);
    data.extend_from_slice(b"data: ");
    data.extend_from_slice(&payload);
    data.extend_from_slice(b"\n\n");
    Some(Bytes::from(data))
}

pub(super) fn sse_claude_bytes(event: &BetaStreamEvent) -> Option<Bytes> {
    let payload = serde_json::to_vec(event).ok()?;
    let mut data = Vec::with_capacity(payload.len() + 32);
    if let Some(name) = claude_event_name(event) {
        data.extend_from_slice(b"event: ");
        data.extend_from_slice(name.as_bytes());
        data.extend_from_slice(b"\n");
    }
    data.extend_from_slice(b"data: ");
    data.extend_from_slice(&payload);
    data.extend_from_slice(b"\n\n");
    Some(Bytes::from(data))
}

fn claude_event_name(event: &BetaStreamEvent) -> Option<&'static str> {
    match event {
        BetaStreamEvent::Known(kind) => Some(match kind {
            BetaStreamEventKnown::MessageStart { .. } => "message_start",
            BetaStreamEventKnown::ContentBlockStart { .. } => "content_block_start",
            BetaStreamEventKnown::ContentBlockDelta { .. } => "content_block_delta",
            BetaStreamEventKnown::ContentBlockStop { .. } => "content_block_stop",
            BetaStreamEventKnown::MessageDelta { .. } => "message_delta",
            BetaStreamEventKnown::MessageStop => "message_stop",
            BetaStreamEventKnown::Ping => "ping",
            BetaStreamEventKnown::Error { .. } => "error",
        }),
        BetaStreamEvent::Unknown(_) => None,
    }
}

pub(super) fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

pub(super) fn parse_gemini_stream_payload(
    data: &str,
) -> Vec<gemini::generate_content::response::GenerateContentResponse> {
    if let Ok(parsed) =
        serde_json::from_str::<gemini::generate_content::response::GenerateContentResponse>(data)
    {
        return vec![parsed];
    }
    if let Ok(parsed) =
        serde_json::from_str::<Vec<gemini::generate_content::response::GenerateContentResponse>>(
            data,
        )
    {
        return parsed;
    }
    Vec::new()
}

#[derive(Debug)]
enum StreamDecoderMode {
    Unknown,
    Sse(SseParser),
    Ndjson(String),
    JsonArray(JsonArrayDecoder),
}

#[derive(Debug)]
pub(super) struct StreamDecoder {
    mode: StreamDecoderMode,
    pending: String,
}

impl StreamDecoder {
    pub(super) fn new() -> Self {
        Self {
            mode: StreamDecoderMode::Unknown,
            pending: String::new(),
        }
    }

    pub(super) fn push(&mut self, chunk: &Bytes) -> Vec<String> {
        let text = match std::str::from_utf8(chunk) {
            Ok(value) => value,
            Err(_) => return Vec::new(),
        };

        match &mut self.mode {
            StreamDecoderMode::Unknown => {
                self.pending.push_str(text);
                let combined = self.pending.as_str();
                let first_non_ws = combined.chars().find(|c| !c.is_whitespace());
                if combined.contains("data:")
                    || combined.contains("event:")
                    || combined.starts_with(':')
                    || matches!(first_non_ws, Some('d' | 'e' | ':'))
                {
                    let mut parser = SseParser::new();
                    let events = parser.push_str(combined);
                    self.mode = StreamDecoderMode::Sse(parser);
                    self.pending.clear();
                    return events
                        .into_iter()
                        .filter_map(|event| {
                            if event.data.is_empty() {
                                None
                            } else {
                                Some(event.data)
                            }
                        })
                        .collect();
                }
                if matches!(first_non_ws, Some('[')) {
                    let mut parser = JsonArrayDecoder::new();
                    let events = parser.push_str(combined);
                    self.mode = StreamDecoderMode::JsonArray(parser);
                    self.pending.clear();
                    return events;
                }
                if matches!(first_non_ws, Some('{')) {
                    let mut buffer = String::new();
                    buffer.push_str(combined);
                    let events = drain_ndjson(&mut buffer);
                    self.mode = StreamDecoderMode::Ndjson(buffer);
                    self.pending.clear();
                    return events;
                }
                if first_non_ws.is_none() {
                    return Vec::new();
                }
                Vec::new()
            }
            StreamDecoderMode::Sse(parser) => parser
                .push_str(text)
                .into_iter()
                .filter_map(|event| {
                    if event.data.is_empty() {
                        None
                    } else {
                        Some(event.data)
                    }
                })
                .collect(),
            StreamDecoderMode::Ndjson(buffer) => {
                buffer.push_str(text);
                drain_ndjson(buffer)
            }
            StreamDecoderMode::JsonArray(parser) => parser.push_str(text),
        }
    }

    pub(super) fn finish(&mut self) -> Vec<String> {
        match &mut self.mode {
            StreamDecoderMode::Unknown => {
                let pending = self.pending.trim();
                if pending.is_empty() {
                    Vec::new()
                } else {
                    vec![pending.to_string()]
                }
            }
            StreamDecoderMode::Sse(parser) => parser
                .finish()
                .into_iter()
                .filter_map(|event| {
                    if event.data.is_empty() {
                        None
                    } else {
                        Some(event.data)
                    }
                })
                .collect(),
            StreamDecoderMode::Ndjson(buffer) => {
                let mut events = drain_ndjson(buffer);
                let remainder = buffer.trim();
                if !remainder.is_empty() {
                    events.push(remainder.to_string());
                }
                buffer.clear();
                events
            }
            StreamDecoderMode::JsonArray(parser) => parser.finish(),
        }
    }
}

#[derive(Debug)]
struct JsonArrayDecoder {
    current: String,
    depth: usize,
    in_string: bool,
    escape: bool,
    seen_array: bool,
}

impl JsonArrayDecoder {
    fn new() -> Self {
        Self {
            current: String::new(),
            depth: 0,
            in_string: false,
            escape: false,
            seen_array: false,
        }
    }

    fn push_str(&mut self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for ch in text.chars() {
            if !self.seen_array {
                if ch.is_whitespace() {
                    continue;
                }
                if ch == '[' {
                    self.seen_array = true;
                }
                continue;
            }

            if self.depth == 0 {
                if ch.is_whitespace() || ch == ',' {
                    continue;
                }
                if ch == '{' {
                    self.depth = 1;
                    self.current.push(ch);
                }
                continue;
            }

            self.current.push(ch);
            if self.in_string {
                if self.escape {
                    self.escape = false;
                } else if ch == '\\' {
                    self.escape = true;
                } else if ch == '"' {
                    self.in_string = false;
                }
                continue;
            }

            match ch {
                '"' => self.in_string = true,
                '{' => self.depth += 1,
                '}' => {
                    self.depth -= 1;
                    if self.depth == 0
                        && !self.current.is_empty() {
                            out.push(std::mem::take(&mut self.current));
                        }
                }
                _ => {}
            }
        }
        out
    }

    fn finish(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if self.depth == 0 && !self.current.trim().is_empty() {
            out.push(std::mem::take(&mut self.current));
        }
        out
    }
}

fn drain_ndjson(buffer: &mut String) -> Vec<String> {
    let mut out = Vec::new();
    loop {
        let Some(pos) = buffer.find('\n') else {
            break;
        };
        let mut line = buffer[..pos].to_string();
        buffer.drain(..=pos);
        if line.ends_with('\r') {
            line.pop();
        }
        let line = line.trim();
        if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    out
}
