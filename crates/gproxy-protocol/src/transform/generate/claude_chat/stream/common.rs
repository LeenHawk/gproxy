use super::*;

#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    pub max_events: usize,
    pub max_bytes: usize,
    pub max_blocks: usize,
    pub max_tools: usize,
}

impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            max_events: 100_000,
            max_bytes: 16 * 1024 * 1024,
            max_blocks: 4096,
            max_tools: 4096,
        }
    }
}

pub struct StreamEnd<T> {
    pub chunks: Vec<T>,
    pub identities: IdentityFlow,
    pub report: Report,
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
    pub fn input<T: serde::Serialize>(&mut self, v: &T) -> Result<(), TransformError> {
        if self.input_events >= self.limits.max_events {
            return Err(limit());
        }
        self.input_events += 1;
        self.input_bytes += bounded(v, self.limits.max_bytes.saturating_sub(self.input_bytes))?;
        Ok(())
    }
    pub fn emit<T: serde::Serialize>(
        &mut self,
        out: &mut Vec<T>,
        v: T,
    ) -> Result<(), TransformError> {
        if self.output_events >= self.limits.max_events {
            return Err(limit());
        }
        self.output_events += 1;
        self.output_bytes += bounded(&v, self.limits.max_bytes.saturating_sub(self.output_bytes))?;
        out.push(v);
        Ok(())
    }
}

pub(super) fn bounded<T: serde::Serialize>(value: &T, cap: usize) -> Result<usize, TransformError> {
    let n = cap as u64;
    crate::codec::encode_json(
        value,
        crate::codec::CodecLimits {
            max_body_bytes: n,
            max_value_bytes: n,
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
            invalid("invalid declared event")
        }
    })
}

pub(super) fn id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    role: IdentityRole,
    source: crate::Dialect,
    value: Option<String>,
    index: u64,
) -> Result<String, TransformError> {
    flow.resolve_or_allocate(role, SourceIdentity::new(source, value, index), policy)
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

/// A legacy Chat `function_call` has no ID; its alias records that it is one.
pub(super) fn legacy_id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    index: u64,
) -> Result<String, TransformError> {
    flow.resolve_legacy_chat_call(
        SourceIdentity::new(crate::Dialect::OpenAiChat, None, index),
        policy,
    )
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

pub(super) fn claude_policy() -> TargetIdPolicy {
    TargetIdPolicy::new(crate::Dialect::Claude)
        .with_generated_prefix(IdentityRole::Response, KnownIdPrefix::Message)
        .with_generated_prefix(IdentityRole::ToolCall, KnownIdPrefix::Tool)
}

pub(super) fn chat_finish(reason: c::StopReason) -> Result<o::FinishReason, TransformError> {
    Ok(match reason {
        c::StopReason::EndTurn | c::StopReason::StopSequence => o::FinishReason::Stop,
        c::StopReason::MaxTokens | c::StopReason::ModelContextWindowExceeded => {
            o::FinishReason::Length
        }
        c::StopReason::ToolUse => o::FinishReason::ToolCalls,
        c::StopReason::Refusal => o::FinishReason::ContentFilter,
        c::StopReason::PauseTurn | c::StopReason::Compaction => o::FinishReason::Stop,
    })
}

pub(super) fn claude_finish(reason: o::FinishReason, refusal: bool) -> c::StopReason {
    match reason {
        o::FinishReason::Stop if refusal => c::StopReason::Refusal,
        o::FinishReason::Stop => c::StopReason::EndTurn,
        o::FinishReason::Length => c::StopReason::MaxTokens,
        o::FinishReason::ToolCalls | o::FinishReason::FunctionCall => c::StopReason::ToolUse,
        o::FinishReason::ContentFilter => c::StopReason::Refusal,
    }
}
