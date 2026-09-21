//! Per-unit rewriting of streamed bodies without re-serializing what did not
//! change. Units are complete SSE events, JSON array elements or NDJSON
//! records, never network chunks. Frames pass through byte for byte unless a
//! rule changed their payload; for SSE only the `data:` lines are replaced,
//! so comments, `event:`, `id:` and `retry:` lines and `[DONE]` stay intact.

use super::{RewriteError, apply_unit};
use crate::RewriteRuleData;
use gproxy_protocol::connection::StreamFraming;
use std::sync::Arc;

pub struct StreamRewriter {
    rules: Vec<Arc<RewriteRuleData>>,
    framing: StreamFraming,
    max_unit_bytes: u64,
    buffer: Vec<u8>,
    array: ArrayState,
}

#[derive(Default)]
struct ArrayState {
    started: bool,
    finished: bool,
    emitted: usize,
    depth: usize,
    in_string: bool,
    escaped: bool,
    /// Byte offset in `buffer` where the current element starts.
    element_start: Option<usize>,
}

impl StreamRewriter {
    /// `rules` are the body rules already selected for this response; an empty
    /// list is the caller's cue not to construct a rewriter at all.
    pub fn new(
        rules: Vec<Arc<RewriteRuleData>>,
        framing: StreamFraming,
        max_unit_bytes: u64,
    ) -> Self {
        Self {
            rules,
            framing,
            max_unit_bytes,
            buffer: Vec::new(),
            array: ArrayState::default(),
        }
    }

    /// Feed one transport chunk; returns the bytes ready to forward.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, RewriteError> {
        self.buffer.extend_from_slice(chunk);
        match self.framing {
            StreamFraming::Sse => self.drain_sse(),
            StreamFraming::NdJson => self.drain_ndjson(),
            StreamFraming::JsonArray => self.drain_array(),
        }
    }

    /// End of stream: an incomplete trailing unit is forwarded unchanged so the
    /// client sees exactly what the upstream sent.
    pub fn finish(&mut self) -> Result<Vec<u8>, RewriteError> {
        Ok(std::mem::take(&mut self.buffer))
    }

    fn check_limit(&self) -> Result<(), RewriteError> {
        if self.buffer.len() as u64 > self.max_unit_bytes {
            return Err(RewriteError::UnitTooLarge(self.max_unit_bytes));
        }
        Ok(())
    }

    fn drain_sse(&mut self) -> Result<Vec<u8>, RewriteError> {
        let mut out = Vec::new();
        loop {
            let Some((end, terminator_len)) = find_sse_terminator(&self.buffer) else {
                self.check_limit()?;
                return Ok(out);
            };
            let frame: Vec<u8> = self.buffer.drain(..end + terminator_len).collect();
            out.extend(rewrite_sse_frame(&self.rules, &frame)?);
        }
    }

    fn drain_ndjson(&mut self) -> Result<Vec<u8>, RewriteError> {
        let mut out = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=newline).collect();
            let (body, ending) = split_line_ending(&line);
            match std::str::from_utf8(body)
                .ok()
                .filter(|text| !text.trim().is_empty())
            {
                Some(text) => match apply_unit(&self.rules, json_type(text).as_deref(), text)? {
                    Some(rewritten) => {
                        out.extend_from_slice(rewritten.as_bytes());
                        out.extend_from_slice(ending);
                    }
                    None => out.extend_from_slice(&line),
                },
                None => out.extend_from_slice(&line),
            }
        }
        self.check_limit()?;
        Ok(out)
    }

    /// Scan for complete top-level elements. Element bytes are kept raw; only
    /// the framing punctuation between them is regenerated.
    fn drain_array(&mut self) -> Result<Vec<u8>, RewriteError> {
        let mut out = Vec::new();
        let mut consumed = 0;
        let mut i = 0;
        while i < self.buffer.len() {
            let byte = self.buffer[i];
            let state = &mut self.array;
            if state.finished {
                out.extend_from_slice(&self.buffer[i..]);
                consumed = self.buffer.len();
                break;
            }
            if !state.started {
                if byte == b'[' {
                    state.started = true;
                    out.push(b'[');
                    consumed = i + 1;
                } else if !byte.is_ascii_whitespace() {
                    return Err(RewriteError::MalformedJsonArray);
                } else {
                    consumed = i + 1;
                }
                i += 1;
                continue;
            }
            match state.element_start {
                None => {
                    if byte == b']' {
                        state.finished = true;
                        out.push(b']');
                        consumed = i + 1;
                    } else if byte == b',' || byte.is_ascii_whitespace() {
                        consumed = i + 1;
                    } else {
                        state.element_start = Some(i);
                        state.depth = 0;
                        state.in_string = false;
                        state.escaped = false;
                        continue;
                    }
                }
                Some(start) => {
                    let mut element_end = None;
                    if state.in_string {
                        if state.escaped {
                            state.escaped = false;
                        } else if byte == b'\\' {
                            state.escaped = true;
                        } else if byte == b'"' {
                            state.in_string = false;
                            if state.depth == 0 {
                                element_end = Some(i + 1);
                            }
                        }
                    } else {
                        match byte {
                            b'"' => state.in_string = true,
                            b'{' | b'[' => state.depth += 1,
                            b'}' | b']' if state.depth > 0 => {
                                state.depth -= 1;
                                if state.depth == 0 {
                                    element_end = Some(i + 1);
                                }
                            }
                            b',' | b']' if state.depth == 0 => element_end = Some(i),
                            b if b.is_ascii_whitespace() && state.depth == 0 => {
                                element_end = Some(i)
                            }
                            _ => {}
                        }
                    }
                    if let Some(end) = element_end {
                        let element = &self.buffer[start..end];
                        if state.emitted > 0 {
                            out.push(b',');
                        }
                        state.emitted += 1;
                        match std::str::from_utf8(element).ok() {
                            Some(text) => {
                                match apply_unit(&self.rules, json_type(text).as_deref(), text)? {
                                    Some(rewritten) => out.extend_from_slice(rewritten.as_bytes()),
                                    None => out.extend_from_slice(element),
                                }
                            }
                            None => out.extend_from_slice(element),
                        }
                        state.element_start = None;
                        consumed = end;
                        i = end;
                        continue;
                    }
                }
            }
            i += 1;
        }
        // Keep an in-progress element; drop everything already forwarded.
        let keep_from = match self.array.element_start {
            Some(start) => start.min(consumed),
            None => consumed,
        };
        self.buffer.drain(..keep_from);
        if let Some(start) = &mut self.array.element_start {
            *start -= keep_from;
        }
        self.check_limit()?;
        Ok(out)
    }
}

fn find_sse_terminator(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < buffer.len() {
        if buffer[i..].starts_with(b"\n\n") {
            return Some((i, 2));
        }
        if buffer[i..].starts_with(b"\r\n\r\n") {
            return Some((i, 4));
        }
        i += 1;
    }
    None
}

fn split_line_ending(line: &[u8]) -> (&[u8], &[u8]) {
    if line.ends_with(b"\r\n") {
        line.split_at(line.len() - 2)
    } else if line.ends_with(b"\n") {
        line.split_at(line.len() - 1)
    } else {
        (line, &[])
    }
}

fn json_type(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| value.get("type")?.as_str().map(str::to_owned))
}

/// One complete frame including its terminator. Only `data:` lines are ever
/// replaced; a non-UTF-8 frame or a `[DONE]` marker passes through untouched.
fn rewrite_sse_frame(
    rules: &[Arc<RewriteRuleData>],
    frame: &[u8],
) -> Result<Vec<u8>, RewriteError> {
    let Ok(text) = std::str::from_utf8(frame) else {
        return Ok(frame.to_vec());
    };
    let crlf = text.contains("\r\n");
    let lines: Vec<&str> = text.trim_end_matches(['\r', '\n']).split('\n').collect();
    let mut data_lines = Vec::new();
    let mut event = None;
    for line in &lines {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        } else if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.strip_prefix(' ').unwrap_or(value).to_owned());
        }
    }
    if data_lines.is_empty() {
        return Ok(frame.to_vec());
    }
    let data = data_lines.join("\n");
    if data == "[DONE]" {
        return Ok(frame.to_vec());
    }
    let event_type = event.or_else(|| json_type(&data));
    let Some(rewritten) = apply_unit(rules, event_type.as_deref(), &data)? else {
        return Ok(frame.to_vec());
    };
    let ending = if crlf { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(frame.len() + rewritten.len());
    let mut emitted_data = false;
    for line in &lines {
        let bare = line.strip_suffix('\r').unwrap_or(line);
        if bare.starts_with("data:") {
            if !emitted_data {
                for piece in rewritten.split('\n') {
                    out.push_str("data: ");
                    out.push_str(piece);
                    out.push_str(ending);
                }
                emitted_data = true;
            }
            continue;
        }
        out.push_str(bare);
        out.push_str(ending);
    }
    out.push_str(ending);
    Ok(out.into_bytes())
}
