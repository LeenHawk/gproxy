use super::*;

#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    pub max_bytes: usize,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Final chunks and the invocation's identity associations. A successful Chat
/// target finish is followed by the SSE `[DONE]` framing token.
pub struct StreamEnd<T> {
    pub chunks: Vec<T>,
    pub identities: IdentityFlow,
    pub report: Report,
}

pub(super) struct Budget {
    limits: StreamLimits,

    bytes: usize,

    output_bytes: usize,
}

impl Budget {
    pub fn new(limits: StreamLimits) -> Self {
        Self {
            limits,

            bytes: 0,

            output_bytes: 0,
        }
    }
    pub fn input<T: serde::Serialize>(&mut self, value: &T) -> Result<(), TransformError> {
        self.bytes += bounded(value, self.limits.max_bytes.saturating_sub(self.bytes))?;
        Ok(())
    }
    pub fn output<T: serde::Serialize>(&mut self, values: &[T]) -> Result<(), TransformError> {
        for value in values {
            self.output_bytes += bounded(
                value,
                self.limits.max_bytes.saturating_sub(self.output_bytes),
            )?;
        }
        Ok(())
    }
}

pub(super) fn bounded<T: serde::Serialize>(value: &T, cap: usize) -> Result<usize, TransformError> {
    let cap = cap as u64;
    crate::codec::encode_json(
        value,
        crate::codec::CodecLimits {
            max_body_bytes: cap,
            max_value_bytes: cap,
            max_buffer_bytes: cap,
            max_line_bytes: cap,
            max_part_bytes: cap,
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

pub(super) fn chat_finish(
    v: g::FinishReason,
    has_tools: bool,
) -> Result<c::FinishReason, TransformError> {
    Ok(match v {
        g::FinishReason::Stop => {
            if has_tools {
                c::FinishReason::ToolCalls
            } else {
                c::FinishReason::Stop
            }
        }
        g::FinishReason::MaxTokens => c::FinishReason::Length,
        g::FinishReason::Safety
        | g::FinishReason::Recitation
        | g::FinishReason::Language
        | g::FinishReason::Blocklist
        | g::FinishReason::ProhibitedContent
        | g::FinishReason::Spii
        | g::FinishReason::ImageSafety
        | g::FinishReason::ImageProhibitedContent
        | g::FinishReason::ImageRecitation
        | g::FinishReason::Escalation => c::FinishReason::ContentFilter,
        g::FinishReason::Unspecified
        | g::FinishReason::Other
        | g::FinishReason::MalformedFunctionCall
        | g::FinishReason::ImageOther
        | g::FinishReason::NoImage
        | g::FinishReason::UnexpectedToolCall
        | g::FinishReason::TooManyToolCalls
        | g::FinishReason::MissingThoughtSignature
        | g::FinishReason::MalformedResponse
        | g::FinishReason::PupLimitedDisabled => {
            return Err(invalid(
                "Gemini ended with an unrepresentable failure reason",
            ));
        }
    })
}

pub(super) fn gemini_finish(v: c::FinishReason) -> g::FinishReason {
    match v {
        c::FinishReason::Stop | c::FinishReason::ToolCalls | c::FinishReason::FunctionCall => {
            g::FinishReason::Stop
        }
        c::FinishReason::Length => g::FinishReason::MaxTokens,
        c::FinishReason::ContentFilter => g::FinishReason::Safety,
    }
}

pub(super) fn id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    role: IdentityRole,
    dialect: crate::Dialect,
    value: Option<String>,
    index: u64,
) -> Result<String, TransformError> {
    flow.resolve_or_allocate(role, SourceIdentity::new(dialect, value, index), policy)
        .map(|v| v.emitted_id)
        .map_err(|e| {
            TransformError::with_source(
                TransformErrorKind::Conflict,
                "stream.identity",
                "identity association failed",
                e,
            )
        })
}
