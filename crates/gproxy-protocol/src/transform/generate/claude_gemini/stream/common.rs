use crate::{
    transform::{
        Report, TransformError, TransformErrorKind,
        identity::{IdentityFlow, IdentityRole, KnownIdPrefix, SourceIdentity, TargetIdPolicy},
    },
    wire::gemini as g,
};

#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    pub max_bytes: usize,
    /// Retained payload/event bytes while waiting for facts or earlier blocks.
    pub max_pending: usize,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_pending: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct StreamEnd<T> {
    pub chunks: Vec<T>,
    pub identities: IdentityFlow,
    pub report: Report,
}

pub(super) fn invalid(field: &str, message: impl Into<String>) -> TransformError {
    TransformError::invalid_result(field, message)
}

pub(super) fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "claude_gemini.stream",
        "stream limit exceeded",
    )
}

pub(super) struct Budget {
    limits: StreamLimits,

    input_bytes: usize,

    output_bytes: usize,
}

impl Budget {
    pub fn new(limits: StreamLimits) -> Self {
        Self {
            limits,

            input_bytes: 0,

            output_bytes: 0,
        }
    }
    pub fn input<T: serde::Serialize>(&mut self, value: &T) -> Result<(), TransformError> {
        self.input_bytes += bound(
            value,
            self.limits.max_bytes.saturating_sub(self.input_bytes),
        )?;
        Ok(())
    }
    /// Charge exactly once, before cloning or retaining an output event.
    pub fn output<T: serde::Serialize>(&mut self, value: &T) -> Result<usize, TransformError> {
        let size = bound(
            value,
            self.limits.max_bytes.saturating_sub(self.output_bytes),
        )?;
        self.output_bytes += size;
        Ok(size)
    }
}

pub(super) fn bound<T: serde::Serialize>(value: &T, cap: usize) -> Result<usize, TransformError> {
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
            invalid("stream.event", e.to_string())
        }
    })
}

pub(super) fn claude_policy() -> TargetIdPolicy {
    TargetIdPolicy::new(crate::Dialect::Claude)
        .with_generated_prefix(IdentityRole::Response, KnownIdPrefix::Message)
        .with_generated_prefix(IdentityRole::ToolCall, KnownIdPrefix::Tool)
}

pub(super) fn response_id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    dialect: crate::Dialect,
    value: Option<String>,
) -> Result<String, TransformError> {
    flow.resolve_or_allocate(
        IdentityRole::Response,
        SourceIdentity::new(dialect, value, 0),
        policy,
    )
    .map(|v| v.emitted_id)
    .map_err(|e| invalid("stream.identity", e.to_string()))
}

/// Native Gemini appends parts. Only boundaries between adjacent plain text
/// fragments with the exact same thought flag are immaterial to this pair.
pub(super) fn normalize_gemini(body: &mut g::GenerateContentResponseBody) {
    for candidate in body.candidates.iter_mut().flatten() {
        if let Some(parts) = candidate.content.as_mut().and_then(|v| v.parts.as_mut()) {
            let mut out: Vec<g::Part> = Vec::new();
            for mut part in std::mem::take(parts) {
                if plain(&part)
                    && let Some(previous) = out.last_mut()
                    && plain(previous)
                    && previous.thought == part.thought
                {
                    previous
                        .text
                        .as_mut()
                        .unwrap()
                        .push_str(part.text.take().unwrap().as_str());
                } else {
                    out.push(part);
                }
            }
            *parts = out;
        }
    }
}

fn plain(part: &g::Part) -> bool {
    part.text.is_some()
        && part.inline_data.is_none()
        && part.file_data.is_none()
        && part.function_call.is_none()
        && part.function_response.is_none()
        && part.executable_code.is_none()
        && part.code_execution_result.is_none()
        && part.tool_call.is_none()
        && part.tool_response.is_none()
        && part.video_metadata.is_none()
        && part.media_processing.is_none()
        && part.audio_transcription.is_none()
        && part.speech_metadata.is_none()
        && part.media_resolution.is_none()
        && part.thought_signature.is_none()
        && part.part_metadata.is_none()
}
