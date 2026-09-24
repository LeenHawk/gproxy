use crate::{
    transform::{Report, TransformError, TransformErrorKind},
    wire::DeclaredFields,
};

#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    pub max_events: usize,
    pub max_bytes: usize,
    pub max_items: usize,
    pub max_tool_calls: usize,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            max_events: usize::MAX,
            max_bytes: usize::MAX,
            max_items: usize::MAX,
            max_tool_calls: usize::MAX,
        }
    }
}

pub struct StreamEnd<T> {
    pub chunks: Vec<T>,
    pub identities: crate::transform::identity::IdentityFlow,
    pub report: Report,
}

pub(super) fn invalid(message: &'static str) -> TransformError {
    TransformError::invalid_result("chat_responses.stream", message)
}

pub(super) fn unsupported(field: &'static str, message: &'static str) -> TransformError {
    TransformError::unsupported(field, message)
}

pub(super) fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "chat_responses.stream",
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
    pub(super) fn new(limits: StreamLimits) -> Self {
        Self {
            limits,
            input_events: 0,
            input_bytes: 0,
            output_events: 0,
            output_bytes: 0,
        }
    }

    pub(super) fn input<T: serde::Serialize>(&mut self, value: &T) -> Result<(), TransformError> {
        self.input_events = self
            .input_events
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_events)
            .ok_or_else(limit)?;
        self.input_bytes = self
            .input_bytes
            .checked_add(measure(
                value,
                self.limits.max_bytes.saturating_sub(self.input_bytes),
            )?)
            .ok_or_else(limit)?;
        Ok(())
    }

    pub(super) fn output<T: serde::Serialize>(&mut self, value: &T) -> Result<(), TransformError> {
        self.output_events = self
            .output_events
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_events)
            .ok_or_else(limit)?;
        self.output_bytes = self
            .output_bytes
            .checked_add(measure(
                value,
                self.limits.max_bytes.saturating_sub(self.output_bytes),
            )?)
            .ok_or_else(limit)?;
        Ok(())
    }
}

pub(super) fn measure<T: serde::Serialize>(
    value: &T,
    remaining: usize,
) -> Result<usize, TransformError> {
    crate::codec::encode_json(
        value,
        crate::codec::CodecLimits {
            max_value_bytes: remaining as u64,
            max_body_bytes: remaining as u64,
            max_buffer_bytes: remaining as u64,
            max_line_bytes: remaining as u64,
            max_part_bytes: remaining as u64,
            max_parts: usize::MAX,
        },
    )
    .map(|v| v.len())
    .map_err(|e| {
        if e.kind() == crate::codec::CodecErrorKind::Limit {
            limit()
        } else {
            invalid("invalid declared stream value")
        }
    })
}

pub(super) fn declared<T: DeclaredFields>(value: T) -> T {
    value.into_declared()
}

pub(super) fn stream_logs(
    logs: &[crate::wire::openai::responses::input::OutputLogprob],
) -> Vec<crate::wire::openai::responses::stream::StreamLogprob> {
    use crate::wire::openai::responses::stream as s;
    logs.iter()
        .map(|v| s::StreamLogprob {
            token: v.token.clone(),
            logprob: v.logprob.clone(),
            top_logprobs: Some(
                v.top_logprobs
                    .iter()
                    .map(|t| s::StreamTopLogprob {
                        token: Some(t.token.clone()),
                        logprob: Some(t.logprob.clone()),
                        rest: Default::default(),
                    })
                    .collect(),
            ),
            rest: Default::default(),
        })
        .collect()
}
