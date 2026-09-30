use crate::{
    transform::{
        Report, TransformError, TransformErrorKind,
        identity::{IdentityFlow, IdentityRole, KnownIdPrefix, SourceIdentity, TargetIdPolicy},
    },
    wire::{DeclaredFields, claude::generate_content as c, openai::responses as r},
};

#[derive(Debug, Clone, Copy)]
pub struct StreamLimits {
    pub max_bytes: usize,
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

pub(super) fn invalid(message: impl Into<String>) -> TransformError {
    TransformError::invalid_result("claude_responses.stream", message)
}

pub(super) fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "claude_responses.stream",
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
        self.input_bytes += measure(
            value,
            self.limits.max_bytes.saturating_sub(self.input_bytes),
        )?;
        Ok(())
    }
    pub fn output<T: serde::Serialize>(&mut self, value: &T) -> Result<usize, TransformError> {
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

pub(super) fn claude_policy() -> TargetIdPolicy {
    TargetIdPolicy::new(crate::Dialect::Claude)
        .with_generated_prefix(IdentityRole::Response, KnownIdPrefix::Message)
        .with_generated_prefix(IdentityRole::ToolCall, KnownIdPrefix::Tool)
}

pub(super) fn id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    dialect: crate::Dialect,
    from: IdentityRole,
    to: IdentityRole,
    source: Option<String>,
    index: u64,
) -> Result<String, TransformError> {
    if source.as_ref().is_some_and(String::is_empty) {
        return Err(invalid("empty native identity"));
    }
    flow.resolve_as(
        from,
        to,
        SourceIdentity::new(dialect, source, index),
        policy,
    )
    .map(|v| v.emitted_id)
    .map_err(|e| invalid(e.to_string()))
}

pub(super) fn item_id(item: &r::ResponseOutputItem) -> Option<&str> {
    match item {
        r::ResponseOutputItem::Message(v) => Some(&v.id),
        r::ResponseOutputItem::Reasoning(v) => Some(&v.id),
        r::ResponseOutputItem::FunctionCall(v) => v.id.as_deref(),
        r::ResponseOutputItem::McpCall(v) => Some(&v.id),
        r::ResponseOutputItem::ShellCall(v) => Some(&v.id),
        r::ResponseOutputItem::ApplyPatchCall(v) => Some(&v.id),
        r::ResponseOutputItem::ToolSearchCall(v) => Some(&v.id),
        _ => None,
    }
}

pub(super) fn normalize_arguments(
    body: &mut r::GenerateContentResponseBody,
) -> Result<(), TransformError> {
    for item in &mut body.output {
        let value = match item {
            r::ResponseOutputItem::FunctionCall(v) => &mut v.arguments,
            r::ResponseOutputItem::McpCall(v) => &mut v.arguments,
            _ => continue,
        };
        let object: serde_json::Map<String, serde_json::Value> = serde_json::from_str(value)
            .map_err(|e| invalid(format!("tool input requires an object: {e}")))?;
        *value = serde_json::to_string(&object)?;
    }
    Ok(())
}

pub(super) fn initial_usage(mut usage: c::Usage) -> Result<c::Usage, TransformError> {
    usage = declared(usage);
    if usage.input_tokens < 0 || usage.output_tokens < 0 {
        return Err(invalid("negative initial usage"));
    }
    for count in [
        usage.cache_read_input_tokens.flatten(),
        usage.cache_creation_input_tokens.flatten(),
    ]
    .into_iter()
    .flatten()
    {
        if count < 0 {
            return Err(invalid("negative initial cache count"));
        }
    }
    if let Some(cache) = usage.cache_creation.as_ref().and_then(Option::as_ref) {
        let total = cache
            .ephemeral_1h_input_tokens
            .checked_add(cache.ephemeral_5m_input_tokens)
            .filter(|_| {
                cache.ephemeral_1h_input_tokens >= 0 && cache.ephemeral_5m_input_tokens >= 0
            })
            .ok_or_else(|| invalid("invalid cache breakdown"))?;
        if usage
            .cache_creation_input_tokens
            .flatten()
            .is_some_and(|n| n != total)
        {
            return Err(invalid("cache breakdown contradicts aggregate"));
        }
        usage.cache_creation_input_tokens = Some(Some(total));
    }
    if usage
        .output_tokens_details
        .as_ref()
        .and_then(Option::as_ref)
        .is_some_and(|v| v.thinking_tokens < 0 || v.thinking_tokens > usage.output_tokens)
    {
        return Err(invalid("invalid initial thinking count"));
    }
    Ok(usage)
}
