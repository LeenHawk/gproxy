use crate::{
    HttpBody, WireResponse,
    adapt::{JsonInvocation, invoke_json},
    capability::Upstream,
    codec::{self, CodecLimits},
    transform::{
        TransformError, TransformErrorKind,
        compact::{self, CompactDialectRequest, CompactDialectRequestBody},
        memory,
    },
    wire::{
        DeclaredFields,
        openai::compact::{ClientCompactRequestBody, ClientCompactResponseBody},
    },
};
#[derive(Debug, Clone, Copy)]
pub struct CompactLimits {
    pub max_bytes: u64,
    pub codec: CodecLimits,
}
#[derive(Debug)]
pub enum CompactFailure {
    Transform(TransformError),
    Rejected(Box<WireResponse<HttpBody>>),
}
#[derive(Debug)]
pub struct CompactError {
    pub attempted_calls: usize,
    pub failure: CompactFailure,
}
impl CompactError {
    pub fn kind(&self) -> TransformErrorKind {
        match &self.failure {
            CompactFailure::Transform(e) => e.kind(),
            CompactFailure::Rejected(_) => TransformErrorKind::InvalidResult,
        }
    }
}
impl From<TransformError> for CompactError {
    fn from(error: TransformError) -> Self {
        Self {
            attempted_calls: 0,
            failure: CompactFailure::Transform(error),
        }
    }
}
fn after(error: TransformError) -> CompactError {
    CompactError {
        attempted_calls: 1,
        failure: CompactFailure::Transform(error),
    }
}
#[allow(clippy::too_many_arguments)]
pub async fn compact<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    input: ClientCompactRequestBody,
    dialect: CompactDialectRequest,
    target_model: &str,
    max_tokens: i64,
    retain_from: usize,
    limits: CompactLimits,
) -> Result<ClientCompactResponseBody, CompactError> {
    let input = input.into_declared();
    let mut local = limits.codec;
    local.max_body_bytes = local.max_body_bytes.min(limits.max_bytes);
    local.max_value_bytes = local.max_value_bytes.min(limits.max_bytes);
    local.max_buffer_bytes = local.max_buffer_bytes.min(limits.max_bytes);
    let prepared =
        compact::build_request(input, dialect, target_model, max_tokens, retain_from, local)?;
    let Some(request) = prepared.request else {
        return compact::replacement_history(None, prepared.context.retained_tail)
            .map_err(CompactError::from);
    };
    let retained = codec::encode_json(&prepared.context.retained_tail, local)
        .map_err(|e| encoding(e, false))?
        .len() as u64;
    let mut write = local;
    write.max_body_bytes = write.max_body_bytes.min(upstream.limits().write_bytes);
    write.max_value_bytes = write.max_value_bytes.min(write.max_body_bytes);
    write.max_buffer_bytes = write.max_buffer_bytes.min(write.max_body_bytes);
    let request_bytes = match &request {
        CompactDialectRequestBody::Claude(v) => codec::encode_json(&v.body, write),
        CompactDialectRequestBody::Gemini(v) => codec::encode_json(&v.body, write),
        CompactDialectRequestBody::OpenAiChat(v) => codec::encode_json(&v.body, write),
        CompactDialectRequestBody::OpenAiResponses(v) => codec::encode_json(&v.body, write),
    }
    .map_err(|e| encoding(e, false))?
    .len() as u64;
    if request_bytes
        .checked_add(retained)
        .is_none_or(|sum| sum > limits.max_bytes)
    {
        return Err(TransformError::new(
            TransformErrorKind::Limit,
            "compact.max_bytes",
            "prepared request plus retained history exceeds bound",
        )
        .into());
    }
    let text = match request {
        CompactDialectRequestBody::Claude(request) => extract(
            invoke_json(upstream, target, request, local).await,
            memory::claude_text,
        )?,
        CompactDialectRequestBody::Gemini(request) => extract(
            invoke_json(upstream, target, request, local).await,
            memory::gemini_text,
        )?,
        CompactDialectRequestBody::OpenAiChat(request) => extract(
            invoke_json(upstream, target, request, local).await,
            memory::chat_text,
        )?,
        CompactDialectRequestBody::OpenAiResponses(request) => extract(
            invoke_json(upstream, target, request, local).await,
            memory::responses_text,
        )?,
    };
    let result =
        compact::replacement_history(Some(text), prepared.context.retained_tail).map_err(after)?;
    codec::encode_json(&result, local).map_err(|e| after(encoding(e, true)))?;
    Ok(result)
}
fn extract<T>(
    response: Result<JsonInvocation<T>, TransformError>,
    text: fn(T) -> Result<String, TransformError>,
) -> Result<String, CompactError> {
    match response.map_err(after)? {
        JsonInvocation::Success(response) => text(response.body).map_err(after),
        JsonInvocation::Rejected(response) => Err(CompactError {
            attempted_calls: 1,
            failure: CompactFailure::Rejected(Box::new(response)),
        }),
    }
}
fn encoding(e: codec::CodecError, response: bool) -> TransformError {
    TransformError::new(
        if e.kind() == codec::CodecErrorKind::Limit {
            TransformErrorKind::Limit
        } else if response {
            TransformErrorKind::InvalidResult
        } else {
            TransformErrorKind::InvalidInput
        },
        "compact.bytes",
        e.to_string(),
    )
}
