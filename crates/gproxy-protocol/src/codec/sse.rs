use super::{CodecError, CodecErrorKind, CodecErrorStage, CodecLimits};
use bytes::Bytes;

/// Parsed SSE data event. `id` and `retry` are effective connection settings,
/// including values supplied in earlier control-only blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub id: Option<String>,
    pub data: String,
    pub retry: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseFrame {
    Event(SseEvent),
    Done,
}

fn invalid(stage: CodecErrorStage, msg: &'static str) -> CodecError {
    CodecError::new(CodecErrorKind::Invalid, stage, msg)
}

fn limit(n: u64, max: u64) -> Result<(), CodecError> {
    if n > max {
        Err(CodecError::new(
            CodecErrorKind::Limit,
            CodecErrorStage::Stream,
            "SSE size limit exceeded",
        ))
    } else {
        Ok(())
    }
}

fn text(bytes: &[u8]) -> Result<&str, CodecError> {
    std::str::from_utf8(bytes).map_err(|e| {
        CodecError::with_source(
            CodecErrorKind::Utf8,
            CodecErrorStage::Stream,
            "invalid SSE UTF-8",
            e,
        )
    })
}

/// Incremental LF/CRLF/CR framing. Invalid UTF-8 is rejected rather than
/// replaced. `finish` rejects a pending data event instead of dispatching an
/// event without its required blank-line delimiter. A final partial comment or
/// field line is also rejected. No vendor lifecycle is inferred from EOF.
pub struct SseDecoder {
    pending: Vec<u8>,
    data: Vec<u8>,
    event: Option<String>,
    id: Option<String>,
    retry: Option<u64>,
    has_data: bool,
    first_line: bool,
    skip_lf: bool,
    seen: u64,
    failed: bool,
    finished: bool,
    limits: CodecLimits,
}

impl SseDecoder {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            pending: Vec::new(),
            data: Vec::new(),
            event: None,
            id: None,
            retry: None,
            has_data: false,
            first_line: true,
            skip_lf: false,
            seen: 0,
            failed: false,
            finished: false,
            limits,
        }
    }
    pub fn last_event_id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    pub fn retry(&self) -> Option<u64> {
        self.retry
    }
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, CodecError> {
        if self.failed || self.finished {
            return Err(invalid(CodecErrorStage::Stream, "SSE decoder terminated"));
        }
        let result = self.push_inner(chunk);
        if result.is_err() {
            self.failed = true;
            self.pending.clear();
            self.data.clear();
        }
        result
    }
    fn push_inner(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, CodecError> {
        self.seen = self.seen.saturating_add(chunk.len() as u64);
        limit(self.seen, self.limits.max_body_bytes)?;
        let mut frames = Vec::new();
        for &b in chunk {
            if self.skip_lf {
                self.skip_lf = false;
                if b == b'\n' {
                    continue;
                }
            }
            if b == b'\r' || b == b'\n' {
                let line = std::mem::take(&mut self.pending);
                self.line(&line, &mut frames)?;
                self.skip_lf = b == b'\r';
            } else {
                limit(
                    (self.pending.len() as u64).saturating_add(1),
                    self.limits.max_line_bytes.saturating_add({
                        let bom = [0xef, 0xbb, 0xbf];
                        if !self.first_line {
                            0
                        } else if self.pending.starts_with(&bom) {
                            3
                        } else if self.pending.len() < 3
                            && bom.starts_with(&self.pending)
                            && b == bom[self.pending.len()]
                        {
                            (self.pending.len() + 1) as u64
                        } else {
                            0
                        }
                    }),
                )?;
                limit(
                    (self.pending.len() as u64)
                        .saturating_add(self.data.len() as u64)
                        .saturating_add(1),
                    self.limits.max_buffer_bytes,
                )?;
                self.pending.push(b);
            }
        }
        Ok(frames)
    }
    fn line(&mut self, line: &[u8], frames: &mut Vec<SseFrame>) -> Result<(), CodecError> {
        let line = if self.first_line {
            self.first_line = false;
            line.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(line)
        } else {
            line
        };
        limit(line.len() as u64, self.limits.max_line_bytes)?;
        text(line)?;
        if line.is_empty() {
            return self.dispatch(frames);
        }
        if line.first() == Some(&b':') {
            return Ok(());
        }
        let (name, value) = match line.iter().position(|b| *b == b':') {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line, &[][..]),
        };
        let value = value.strip_prefix(b" ").unwrap_or(value);
        match text(name)? {
            "data" => {
                let n = (self.data.len() as u64)
                    .saturating_add(value.len() as u64)
                    .saturating_add(u64::from(self.has_data));
                limit(
                    n,
                    self.limits
                        .max_value_bytes
                        .min(self.limits.max_buffer_bytes),
                )?;
                if self.has_data {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value);
                self.has_data = true;
            }
            "event" => self.event = Some(text(value)?.to_owned()),
            "id" => {
                let id = text(value)?;
                if !id.contains('\0') {
                    self.id = Some(id.to_owned());
                }
            }
            "retry" => {
                if !value.is_empty()
                    && value.iter().all(u8::is_ascii_digit)
                    && let Ok(retry) = text(value)?.parse()
                {
                    self.retry = Some(retry);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn dispatch(&mut self, frames: &mut Vec<SseFrame>) -> Result<(), CodecError> {
        let event = self.event.take();
        if !self.has_data {
            return Ok(());
        }
        let data = text(&self.data)?.to_owned();
        if data == "[DONE]" {
            frames.push(SseFrame::Done);
        } else {
            frames.push(SseFrame::Event(SseEvent {
                event,
                id: self.id.clone(),
                retry: self.retry,
                data,
            }));
        }
        self.data.clear();
        self.has_data = false;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<Vec<SseFrame>, CodecError> {
        if self.failed || self.finished {
            return Err(invalid(CodecErrorStage::Finish, "SSE decoder terminated"));
        }
        self.finished = true;
        if !self.pending.is_empty() || self.has_data {
            self.failed = true;
            return Err(CodecError::new(
                CodecErrorKind::UnexpectedEof,
                CodecErrorStage::Finish,
                "unterminated SSE data event or line",
            ));
        }
        self.event = None;
        Ok(Vec::new())
    }
}

fn line(
    out: &mut Vec<u8>,
    field: &str,
    value: &str,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    if value.contains(['\r', '\n']) || (field == "id" && value.contains('\0')) {
        return Err(invalid(CodecErrorStage::Start, "invalid SSE field value"));
    }
    let n = (field.len() as u64)
        .saturating_add(2)
        .saturating_add(value.len() as u64);
    limit(n, limits.max_line_bytes)?;
    limit(
        (out.len() as u64).saturating_add(n).saturating_add(1),
        limits.max_body_bytes,
    )?;
    out.extend_from_slice(field.as_bytes());
    out.extend_from_slice(b": ");
    out.extend_from_slice(value.as_bytes());
    out.push(b'\n');
    Ok(())
}

/// Encodes one event. Use SseEncoder for cumulative limits over a whole body.
pub fn encode_sse_event(event: &SseEvent, limits: CodecLimits) -> Result<Bytes, CodecError> {
    limit(event.data.len() as u64, limits.max_value_bytes)?;
    let mut output = Vec::new();
    if let Some(v) = &event.event {
        line(&mut output, "event", v, limits)?;
    }
    if let Some(v) = &event.id {
        line(&mut output, "id", v, limits)?;
    }
    if let Some(v) = event.retry {
        line(&mut output, "retry", &v.to_string(), limits)?;
    }
    for data in event.data.split('\n') {
        line(&mut output, "data", data, limits)?;
    }
    limit(
        (output.len() as u64).saturating_add(1),
        limits.max_body_bytes,
    )?;
    output.push(b'\n');
    Ok(Bytes::from(output))
}

/// Literal marker bytes, not a complete streaming encoder; SseEncoder::done
/// accounts this marker against the enclosing body limit.
pub fn encode_sse_done() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}

pub struct SseEncoder {
    limits: CodecLimits,
    seen: u64,
    terminated: bool,
}

impl SseEncoder {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            limits,
            seen: 0,
            terminated: false,
        }
    }
    pub fn event(&mut self, event: &SseEvent) -> Result<Bytes, CodecError> {
        if self.terminated {
            return Err(invalid(CodecErrorStage::Stream, "SSE encoder terminated"));
        }
        let result = (|| {
            let bytes = encode_sse_event(event, self.limits)?;
            self.account(bytes)
        })();
        if result.is_err() {
            self.terminated = true;
        }
        result
    }
    fn account(&mut self, bytes: Bytes) -> Result<Bytes, CodecError> {
        limit(
            self.seen.saturating_add(bytes.len() as u64),
            self.limits.max_body_bytes,
        )?;
        self.seen += bytes.len() as u64;
        Ok(bytes)
    }
    pub fn done(&mut self) -> Result<Bytes, CodecError> {
        if self.terminated {
            return Err(invalid(CodecErrorStage::Stream, "SSE encoder terminated"));
        }
        self.terminated = true;
        let bytes = encode_sse_event(
            &SseEvent {
                event: None,
                id: None,
                retry: None,
                data: "[DONE]".into(),
            },
            self.limits,
        )?;
        self.account(bytes)
    }
}
