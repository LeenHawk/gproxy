use crate::{
    HttpBody, WireResponse,
    adapt::{JsonInvocation, invoke_json},
    capability::Upstream,
    codec::{self, CodecLimits},
    openai::memory::{MemorySummarizeOutput, RawMemory},
    transform::{
        TransformError, TransformErrorKind,
        memory::{self, MemoryDialectRequest},
    },
    wire::DeclaredFields,
};

#[derive(Debug, Clone, Copy)]
pub struct MemoryLimits {
    pub max_bytes: u64,
    pub codec: CodecLimits,
}

#[derive(Debug, Clone, Copy)]
pub enum MemoryDialect {
    Claude,
    Gemini,
    OpenAiChat,
    OpenAiResponses,
}

#[derive(Debug)]
pub enum MemoryFailure {
    Transform(TransformError),
    Rejected(Box<WireResponse<HttpBody>>),
}

#[derive(Debug)]
pub struct MemoryError {
    pub attempted_calls: usize,
    pub completed_traces: usize,
    pub trace_id: Option<String>,
    pub failure: MemoryFailure,
}

impl MemoryError {
    fn at(error: TransformError, calls: usize, done: usize, id: Option<String>) -> Self {
        Self {
            attempted_calls: calls,
            completed_traces: done,
            trace_id: id,
            failure: MemoryFailure::Transform(error),
        }
    }
    pub fn kind(&self) -> TransformErrorKind {
        match &self.failure {
            MemoryFailure::Transform(e) => e.kind(),
            MemoryFailure::Rejected(_) => TransformErrorKind::InvalidResult,
        }
    }
}

impl From<TransformError> for MemoryError {
    fn from(e: TransformError) -> Self {
        Self::at(e, 0, 0, None)
    }
}

pub async fn summarize<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    dialect: MemoryDialect,
    traces: Vec<RawMemory>,
    model: impl Into<String>,
    max_tokens: i64,
    limits: MemoryLimits,
) -> Result<Vec<MemorySummarizeOutput>, MemoryError> {
    summarize_with_reasoning(
        upstream, target, dialect, traces, model, max_tokens, None, limits,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn summarize_with_reasoning<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    dialect: MemoryDialect,
    traces: Vec<RawMemory>,
    model: impl Into<String>,
    max_tokens: i64,
    reasoning: Option<crate::openai::guardian::Reasoning>,
    limits: MemoryLimits,
) -> Result<Vec<MemorySummarizeOutput>, MemoryError> {
    let model = model.into();
    if model.trim().is_empty() || max_tokens <= 0 {
        return Err(TransformError::shape(
            "memory.target",
            "nonempty model and positive token budget required",
        )
        .into());
    }

    let reasoning = reasoning.into_declared();
    let mut planned = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    let mut total = 0u64;
    let mut retained = 0u64;
    let mut bounds = limits.codec;
    bounds.max_body_bytes = bounds.max_body_bytes.min(upstream.limits().write_bytes);
    bounds.max_value_bytes = bounds.max_value_bytes.min(bounds.max_body_bytes);
    bounds.max_buffer_bytes = bounds.max_buffer_bytes.min(bounds.max_body_bytes);
    // Validate every trace and fully encode-check every target request before the
    // first side effect; a malformed later trace cannot cause partial execution.
    for trace in traces {
        let trace = trace.into_declared();

        if !ids.insert(trace.id.clone()) {
            return Err(TransformError::shape("trace.id", "duplicate trace identity").into());
        }
        let remaining = limits
            .max_bytes
            .checked_sub(total)
            .ok_or_else(|| limit("max_bytes"))?;
        let mut trace_bounds = bounds;
        trace_bounds.max_value_bytes = trace_bounds.max_value_bytes.min(remaining);
        trace_bounds.max_body_bytes = trace_bounds.max_body_bytes.min(remaining);
        let payload = memory::trace_payload(&trace, trace_bounds)?;
        total = total
            .checked_add(payload.len() as u64)
            .ok_or_else(|| limit("max_bytes"))?;
        let request = match dialect {
            MemoryDialect::Claude => {
                memory::build_claude(payload, model.clone(), max_tokens, reasoning.as_ref())?
            }
            MemoryDialect::Gemini => {
                memory::build_gemini(payload, model.clone(), max_tokens, reasoning.as_ref())?
            }
            MemoryDialect::OpenAiChat => {
                memory::build_openai_chat(payload, model.clone(), max_tokens, reasoning.as_ref())?
            }
            MemoryDialect::OpenAiResponses => memory::build_openai_responses(
                payload,
                model.clone(),
                max_tokens,
                reasoning.as_ref(),
            )?,
        };
        let mut request_bounds = bounds;
        let remaining = limits
            .max_bytes
            .checked_sub(retained)
            .ok_or_else(|| limit("max_bytes"))?;
        request_bounds.max_value_bytes = request_bounds.max_value_bytes.min(remaining);
        request_bounds.max_buffer_bytes = request_bounds.max_buffer_bytes.min(remaining);
        request_bounds.max_body_bytes = request_bounds.max_body_bytes.min(remaining);
        let encoded = match &request {
            MemoryDialectRequest::Claude(v) => codec::encode_json(&v.body, request_bounds),
            MemoryDialectRequest::Gemini(v) => codec::encode_json(&v.body, request_bounds),
            MemoryDialectRequest::OpenAiChat(v) => codec::encode_json(&v.body, request_bounds),
            MemoryDialectRequest::OpenAiResponses(v) => codec::encode_json(&v.body, request_bounds),
        }
        .map_err(|e| {
            TransformError::new(
                if e.kind() == codec::CodecErrorKind::Limit {
                    TransformErrorKind::Limit
                } else {
                    TransformErrorKind::InvalidInput
                },
                "memory.request",
                e.to_string(),
            )
        })?;
        // Account before retaining the next typed request; a large batch of
        // tiny traces must not accumulate unbounded repeated schema overhead.
        retained = retained
            .checked_add(encoded.len() as u64)
            .filter(|n| *n <= limits.max_bytes)
            .ok_or_else(|| limit("max_bytes"))?;
        planned.push((trace.id, request));
    }
    let mut output = Vec::new();
    let mut output_bytes = 0u64;
    for (index, (id, request)) in planned.into_iter().enumerate() {
        let text = match request {
            MemoryDialectRequest::Claude(request) => finish(
                invoke_json(upstream, target, request, limits.codec).await,
                memory::claude_text,
                index,
                output.len(),
                &id,
            )?,
            MemoryDialectRequest::Gemini(request) => finish(
                invoke_json(upstream, target, request, limits.codec).await,
                memory::gemini_text,
                index,
                output.len(),
                &id,
            )?,
            MemoryDialectRequest::OpenAiChat(request) => finish(
                invoke_json(upstream, target, request, limits.codec).await,
                memory::chat_text,
                index,
                output.len(),
                &id,
            )?,
            MemoryDialectRequest::OpenAiResponses(request) => finish(
                invoke_json(upstream, target, request, limits.codec).await,
                memory::responses_text,
                index,
                output.len(),
                &id,
            )?,
        };
        let (trace, memory) = memory::parse_output(&text, limits.codec)
            .map_err(|e| MemoryError::at(e, index + 1, output.len(), Some(id.clone())))?;
        let value = MemorySummarizeOutput::builder(trace, memory).build();
        let bytes = codec::encode_json(&value, limits.codec).map_err(|e| {
            MemoryError::at(
                TransformError::new(TransformErrorKind::Limit, "memory.output", e.to_string()),
                index + 1,
                output.len(),
                Some(id.clone()),
            )
        })?;
        output_bytes = output_bytes
            .checked_add(bytes.len() as u64)
            .filter(|v| *v <= limits.max_bytes)
            .ok_or_else(|| {
                MemoryError::at(limit("max_bytes"), index + 1, output.len(), Some(id))
            })?;
        output.push(value);
    }
    Ok(output)
}

fn finish<T>(
    result: Result<JsonInvocation<T>, TransformError>,
    extract: fn(T) -> Result<String, TransformError>,
    index: usize,
    completed: usize,
    id: &str,
) -> Result<String, MemoryError> {
    match result.map_err(|e| MemoryError::at(e, index + 1, completed, Some(id.into())))? {
        JsonInvocation::Success(response) => extract(response.body)
            .map_err(|e| MemoryError::at(e, index + 1, completed, Some(id.into()))),
        JsonInvocation::Rejected(response) => Err(MemoryError {
            attempted_calls: index + 1,
            completed_traces: completed,
            trace_id: Some(id.into()),
            failure: MemoryFailure::Rejected(Box::new(response)),
        }),
    }
}

fn limit(field: &str) -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        format!("memory.{field}"),
        "memory operation bound exceeded",
    )
}
