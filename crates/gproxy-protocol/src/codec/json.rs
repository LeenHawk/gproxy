use super::{CodecError, CodecErrorKind, CodecErrorStage, CodecLimits};
use bytes::Bytes;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::io::{self, Write};

fn error(kind: CodecErrorKind, stage: CodecErrorStage, message: &'static str) -> CodecError {
    CodecError::new(kind, stage, message)
}
fn json_error(source: serde_json::Error, stage: CodecErrorStage) -> CodecError {
    CodecError::with_source(
        if source.is_eof() {
            CodecErrorKind::UnexpectedEof
        } else {
            CodecErrorKind::Json
        },
        stage,
        "invalid JSON",
        source,
    )
}
fn size(n: u64, max: u64) -> Result<(), CodecError> {
    if n > max {
        Err(error(
            CodecErrorKind::Limit,
            CodecErrorStage::Stream,
            "JSON size limit exceeded",
        ))
    } else {
        Ok(())
    }
}
fn whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub struct JsonDecoder {
    buffer: Vec<u8>,
    limits: CodecLimits,
    failed: bool,
}
impl JsonDecoder {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            buffer: Vec::new(),
            limits,
            failed: false,
        }
    }
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), CodecError> {
        if self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Stream,
                "JSON decoder terminated",
            ));
        }
        let n = (self.buffer.len() as u64).saturating_add(chunk.len() as u64);
        if let Err(e) = size(
            n,
            self.limits
                .max_buffer_bytes
                .min(self.limits.max_value_bytes)
                .min(self.limits.max_body_bytes),
        ) {
            self.failed = true;
            return Err(e);
        }
        self.buffer.extend_from_slice(chunk);
        Ok(())
    }
    pub fn finish<T: DeserializeOwned>(self) -> Result<T, CodecError> {
        if self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Finish,
                "JSON decoder terminated",
            ));
        }
        serde_json::from_slice(&self.buffer).map_err(|e| json_error(e, CodecErrorStage::Finish))
    }
}
pub fn decode_json<T: DeserializeOwned>(
    bytes: &[u8],
    limits: CodecLimits,
) -> Result<T, CodecError> {
    let mut d = JsonDecoder::new(limits);
    d.push(bytes)?;
    d.finish()
}
struct LimitedWriter {
    bytes: Vec<u8>,
    max: u64,
    exceeded: bool,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if (self.bytes.len() as u64).saturating_add(bytes.len() as u64) > self.max {
            self.exceeded = true;
            return Err(io::Error::other("JSON value exceeds limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn encode_json<T: Serialize>(value: &T, limits: CodecLimits) -> Result<Bytes, CodecError> {
    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        max: limits.max_value_bytes.min(limits.max_body_bytes),
        exceeded: false,
    };
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(Bytes::from(writer.bytes)),
        Err(_) if writer.exceeded => Err(error(
            CodecErrorKind::Limit,
            CodecErrorStage::Start,
            "JSON value exceeds limit",
        )),
        Err(e) => Err(json_error(e, CodecErrorStage::Start)),
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Start,
    Value { allow_end: bool },
    InValue,
    Separator,
    Done,
    Finished,
    Failed,
}
/// Parses array elements only once a delimiter establishes the token boundary.
/// The retained buffer is one value, independent of transport chunk size.
pub struct JsonArrayDecoder {
    value: Vec<u8>,
    limits: CodecLimits,
    phase: Phase,
    depth: usize,
    string: bool,
    escape: bool,
    seen: u64,
}
impl JsonArrayDecoder {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            value: Vec::new(),
            limits,
            phase: Phase::Start,
            depth: 0,
            string: false,
            escape: false,
            seen: 0,
        }
    }
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Value>, CodecError> {
        if matches!(self.phase, Phase::Failed | Phase::Finished) {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Stream,
                "array decoder terminated",
            ));
        }
        let result = self.push_inner(chunk);
        if result.is_err() {
            self.phase = Phase::Failed;
            self.value.clear();
        }
        result
    }
    fn push_inner(&mut self, chunk: &[u8]) -> Result<Vec<Value>, CodecError> {
        self.seen = self.seen.saturating_add(chunk.len() as u64);
        size(self.seen, self.limits.max_body_bytes)?;
        let mut values = Vec::new();
        for &b in chunk {
            self.byte(b, &mut values)?;
        }
        Ok(values)
    }
    fn byte(&mut self, b: u8, values: &mut Vec<Value>) -> Result<(), CodecError> {
        match self.phase {
            Phase::Start => {
                if whitespace(b) {
                    return Ok(());
                }
                if b != b'[' {
                    return Err(error(
                        CodecErrorKind::Invalid,
                        CodecErrorStage::Stream,
                        "array must start with '['",
                    ));
                }
                self.phase = Phase::Value { allow_end: true };
            }
            Phase::Value { allow_end } => {
                if whitespace(b) {
                    return Ok(());
                }
                if b == b']' && allow_end {
                    self.phase = Phase::Done;
                    return Ok(());
                }
                if b == b']' || b == b',' {
                    return Err(error(
                        CodecErrorKind::Invalid,
                        CodecErrorStage::Stream,
                        "missing array value or trailing comma",
                    ));
                }
                self.phase = Phase::InValue;
                self.scan(b)?;
            }
            Phase::InValue => {
                if !self.string && self.depth == 0 && (whitespace(b) || b == b',' || b == b']') {
                    let value = serde_json::from_slice(&self.value)
                        .map_err(|e| json_error(e, CodecErrorStage::Stream))?;
                    self.value.clear();
                    values.push(value);
                    self.phase = Phase::Separator;
                    self.byte(b, values)?;
                } else {
                    self.scan(b)?;
                }
            }
            Phase::Separator => {
                if whitespace(b) {
                    return Ok(());
                }
                match b {
                    b',' => self.phase = Phase::Value { allow_end: false },
                    b']' => self.phase = Phase::Done,
                    _ => {
                        return Err(error(
                            CodecErrorKind::Invalid,
                            CodecErrorStage::Stream,
                            "array needs comma or ']'",
                        ));
                    }
                }
            }
            Phase::Done => {
                if !whitespace(b) {
                    return Err(error(
                        CodecErrorKind::Invalid,
                        CodecErrorStage::Stream,
                        "data follows array",
                    ));
                }
            }
            Phase::Finished | Phase::Failed => {
                return Err(error(
                    CodecErrorKind::Invalid,
                    CodecErrorStage::Stream,
                    "array decoder terminated",
                ));
            }
        }
        Ok(())
    }
    fn scan(&mut self, b: u8) -> Result<(), CodecError> {
        size(
            (self.value.len() as u64).saturating_add(1),
            self.limits
                .max_value_bytes
                .min(self.limits.max_buffer_bytes),
        )?;
        self.value.push(b);
        if self.string {
            if self.escape {
                self.escape = false;
            } else if b == b'\\' {
                self.escape = true;
            } else if b == b'"' {
                self.string = false;
            }
        } else {
            match b {
                b'"' => self.string = true,
                b'{' | b'[' => self.depth += 1,
                b'}' | b']' => {
                    self.depth = self.depth.checked_sub(1).ok_or_else(|| {
                        error(
                            CodecErrorKind::Invalid,
                            CodecErrorStage::Stream,
                            "unmatched JSON delimiter",
                        )
                    })?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), CodecError> {
        if matches!(self.phase, Phase::Done) {
            self.phase = Phase::Finished;
            Ok(())
        } else {
            let kind = if matches!(self.phase, Phase::Failed | Phase::Finished) {
                CodecErrorKind::Invalid
            } else {
                CodecErrorKind::UnexpectedEof
            };
            self.phase = Phase::Failed;
            self.value.clear();
            Err(error(
                kind,
                CodecErrorStage::Finish,
                "unfinished JSON array",
            ))
        }
    }
}

pub struct JsonArrayEncoder {
    limits: CodecLimits,
    started: bool,
    finished: bool,
    failed: bool,
    seen: u64,
}
impl JsonArrayEncoder {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            limits,
            started: false,
            finished: false,
            failed: false,
            seen: 0,
        }
    }
    pub fn push<T: Serialize>(&mut self, value: &T) -> Result<Bytes, CodecError> {
        if self.finished || self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Stream,
                "array encoder terminated",
            ));
        }
        let result = (|| {
            let encoded = encode_json(value, self.limits)?;
            let n = (encoded.len() as u64).saturating_add(1);
            size(self.seen.saturating_add(n), self.limits.max_body_bytes)?;
            let mut v = Vec::with_capacity(encoded.len() + 1);
            v.push(if self.started { b',' } else { b'[' });
            v.extend_from_slice(&encoded);
            self.seen += n;
            self.started = true;
            Ok(Bytes::from(v))
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn finish(&mut self) -> Result<Bytes, CodecError> {
        if self.finished || self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Finish,
                "array encoder terminated",
            ));
        }
        self.finished = true;
        let ending = if self.started {
            b"]".as_slice()
        } else {
            b"[]".as_slice()
        };
        size(
            self.seen.saturating_add(ending.len() as u64),
            self.limits.max_body_bytes,
        )?;
        Ok(Bytes::copy_from_slice(ending))
    }
}

/// Accepts LF/CRLF and an optional final complete JSON line without LF.
pub struct NdjsonDecoder {
    line: Vec<u8>,
    limits: CodecLimits,
    seen: u64,
    finished: bool,
    failed: bool,
}
impl NdjsonDecoder {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            line: Vec::new(),
            limits,
            seen: 0,
            finished: false,
            failed: false,
        }
    }
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Value>, CodecError> {
        if self.finished || self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Stream,
                "NDJSON decoder terminated",
            ));
        }
        let result = (|| {
            self.seen = self.seen.saturating_add(chunk.len() as u64);
            size(self.seen, self.limits.max_body_bytes)?;
            let mut values = Vec::new();
            for &b in chunk {
                if b == b'\n' {
                    self.flush(&mut values)?;
                } else {
                    size(
                        (self.line.len() as u64).saturating_add(1),
                        self.limits.max_line_bytes.min(self.limits.max_buffer_bytes),
                    )?;
                    self.line.push(b);
                }
            }
            Ok(values)
        })();
        if result.is_err() {
            self.failed = true;
            self.line.clear();
        }
        result
    }
    fn flush(&mut self, values: &mut Vec<Value>) -> Result<(), CodecError> {
        if self.line.last() == Some(&b'\r') {
            self.line.pop();
        }
        if !self.line.iter().all(|b| whitespace(*b)) {
            size(self.line.len() as u64, self.limits.max_value_bytes)?;
            values.push(
                serde_json::from_slice(&self.line)
                    .map_err(|e| json_error(e, CodecErrorStage::Stream))?,
            );
        }
        self.line.clear();
        Ok(())
    }
    pub fn finish(&mut self) -> Result<Vec<Value>, CodecError> {
        if self.finished || self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Finish,
                "NDJSON decoder terminated",
            ));
        }
        self.finished = true;
        let mut values = Vec::new();
        self.flush(&mut values)?;
        Ok(values)
    }
}
pub struct NdjsonEncoder {
    limits: CodecLimits,
    seen: u64,
    failed: bool,
}
impl NdjsonEncoder {
    pub fn new(limits: CodecLimits) -> Self {
        Self {
            limits,
            seen: 0,
            failed: false,
        }
    }
    pub fn encode<T: Serialize>(&mut self, value: &T) -> Result<Bytes, CodecError> {
        if self.failed {
            return Err(error(
                CodecErrorKind::Invalid,
                CodecErrorStage::Stream,
                "NDJSON encoder terminated",
            ));
        }
        let result = (|| {
            let mut v = encode_json(value, self.limits)?.to_vec();
            size(v.len() as u64, self.limits.max_line_bytes)?;
            size(
                self.seen.saturating_add(v.len() as u64).saturating_add(1),
                self.limits.max_body_bytes,
            )?;
            v.push(b'\n');
            self.seen += v.len() as u64;
            Ok(Bytes::from(v))
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
