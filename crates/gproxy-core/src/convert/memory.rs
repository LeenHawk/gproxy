//! Codex memory trace summarisation: one generation call per trace on the
//! target dialect, in submitted order, with the structured summaries
//! returned as the client's `output` array. A failed trace ends the request
//! without presenting the earlier summaries as a partial success.

use super::{Call, Converted};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    adapt::memory::{
        MemoryDialect, MemoryError, MemoryFailure, MemoryLimits, summarize_with_reasoning,
    },
    codec::{CodecLimits, decode_json, encode_json},
    openai::memory::{MemorySummarizeRequestBody, MemorySummarizeResponseBody},
    transform::{TransformError, TransformErrorKind},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE};

/// Output budget per trace summary.
const SUMMARY_MAX_TOKENS: i64 = 2048;
/// Upper bound on traces, and therefore native calls, per client request.
const MAX_CALLS: usize = 64;
/// Upper bound on items inside one trace.
const MAX_TRACE_ITEMS: usize = 512;

fn codec(error: gproxy_protocol::codec::CodecError) -> TransformError {
    TransformError::with_source(
        if error.kind() == gproxy_protocol::codec::CodecErrorKind::Limit {
            TransformErrorKind::Limit
        } else {
            TransformErrorKind::InvalidInput
        },
        "client.body",
        error.to_string(),
        error,
    )
}

fn dialect(target: Dialect) -> Result<MemoryDialect, TransformError> {
    Ok(match target {
        Dialect::OpenAi => MemoryDialect::OpenAiResponses,
        Dialect::OpenAiChat => MemoryDialect::OpenAiChat,
        Dialect::Claude => MemoryDialect::Claude,
        Dialect::Gemini => MemoryDialect::Gemini,
        Dialect::OpenAiResponsesWebSocket => {
            return Err(TransformError::unsupported(
                "memory",
                "the Responses WebSocket dialect cannot host summarisation",
            ));
        }
    })
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    if call.client.dialect != Dialect::OpenAi {
        return Err(TransformError::unsupported(
            "memory",
            format!(
                "memory requests are OpenAI-shaped; {:?} clients are not converted",
                call.client.dialect
            ),
        ));
    }
    let limits: CodecLimits = call.limits;
    let input: MemorySummarizeRequestBody = decode_json(call.body(), limits).map_err(codec)?;
    let key = OperationKey {
        operation: Operation::GenerateContent,
        dialect: call.target,
    };
    // Boxed: the adapter's state machine is large and the attempt loop polls
    // this family alongside every other on one stack frame.
    let result = Box::pin(summarize_with_reasoning(
        call.upstream,
        &key,
        dialect(call.target)?,
        input.traces,
        call.model()?,
        SUMMARY_MAX_TOKENS,
        input.reasoning,
        MemoryLimits {
            max_calls: MAX_CALLS,
            max_trace_items: MAX_TRACE_ITEMS,
            max_bytes: limits.max_body_bytes,
            codec: limits,
        },
    ))
    .await;
    let output = match result {
        Ok(output) => output,
        Err(MemoryError {
            failure: MemoryFailure::Rejected(response),
            ..
        }) => return Ok(Converted::Rejected(*response)),
        Err(MemoryError {
            failure: MemoryFailure::Transform(error),
            ..
        }) => return Err(error),
    };
    let body = MemorySummarizeResponseBody::builder(output).build();
    let bytes = encode_json(&body, limits).map_err(codec)?;
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(Converted::Success(WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(bytes),
    }))
}
