use crate::{
    transform::{Report, TransformError, TransformErrorKind, identity::IdentityFlow},
    wire::DeclaredFields,
};

#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    pub max_events: usize,
    pub max_bytes: usize,
    pub max_pending: usize,
    pub max_items: usize,
    pub max_parts: usize,
    pub max_tools: usize,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            max_events: 100_000,
            max_bytes: 16 * 1024 * 1024,
            max_pending: 16 * 1024 * 1024,
            max_items: 4096,
            max_parts: 4096,
            max_tools: 4096,
        }
    }
}

#[derive(Debug)]
pub struct StreamEnd<T> {
    pub chunks: Vec<T>,
    pub identities: IdentityFlow,
    pub report: Report,
}

pub(super) fn invalid(message: impl Into<String>) -> TransformError {
    TransformError::invalid_result("gemini_responses.stream", message)
}

pub(super) fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "gemini_responses.stream",
        "stream limit exceeded",
    )
}

pub(super) struct Budget {
    limits: StreamLimits,
    input_events: usize,
    input_bytes: usize,
    output_events: usize,
    output_bytes: usize,
}

impl Budget {
    pub fn new(limits: StreamLimits) -> Self {
        Self {
            limits,
            input_events: 0,
            input_bytes: 0,
            output_events: 0,
            output_bytes: 0,
        }
    }
    pub fn input<T: serde::Serialize>(&mut self, value: &T) -> Result<(), TransformError> {
        if self.input_events >= self.limits.max_events {
            return Err(limit());
        }
        self.input_events += 1;
        self.input_bytes += measure(
            value,
            self.limits.max_bytes.saturating_sub(self.input_bytes),
        )?;
        Ok(())
    }
    pub fn output<T: serde::Serialize>(&mut self, value: &T) -> Result<usize, TransformError> {
        if self.output_events >= self.limits.max_events {
            return Err(limit());
        }
        self.output_events += 1;
        let n = measure(
            value,
            self.limits.max_bytes.saturating_sub(self.output_bytes),
        )?;
        self.output_bytes += n;
        Ok(n)
    }
}

pub(super) fn measure<T: serde::Serialize>(value: &T, cap: usize) -> Result<usize, TransformError> {
    let n = cap as u64;
    crate::codec::encode_json(
        value,
        crate::codec::CodecLimits {
            max_value_bytes: n,
            max_body_bytes: n,
            max_buffer_bytes: n,
            max_line_bytes: n,
            max_part_bytes: n,
            max_parts: usize::MAX,
        },
    )
    .map(|v| v.len())
    .map_err(|e| {
        if e.kind() == crate::codec::CodecErrorKind::Limit {
            limit()
        } else {
            invalid(e.to_string())
        }
    })
}

pub(super) fn declared<T: DeclaredFields>(value: T) -> T {
    value.into_declared()
}
