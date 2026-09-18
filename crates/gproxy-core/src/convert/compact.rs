//! Codex remote compaction through one generation call on the target
//! dialect. The whole submitted history is summarised and the client gets a
//! replacement history holding that summary as an assistant message. No
//! encrypted compaction item, usage or response identity is invented.

use super::{Call, Converted};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    adapt::compact::{CompactError, CompactFailure, CompactLimits, compact},
    codec::{CodecLimits, decode_json, encode_json},
    transform::{TransformError, TransformErrorKind, compact::CompactDialectRequest},
    wire::openai::compact::ClientCompactRequestBody,
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE};

/// Output budget for the summary the target model writes.
const SUMMARY_MAX_TOKENS: i64 = 4096;

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

fn dialect(target: Dialect) -> Result<CompactDialectRequest, TransformError> {
    Ok(match target {
        Dialect::OpenAi => CompactDialectRequest::Responses,
        Dialect::OpenAiChat => CompactDialectRequest::Chat,
        Dialect::Claude => CompactDialectRequest::Claude,
        Dialect::Gemini => CompactDialectRequest::Gemini,
        Dialect::OpenAiResponsesWebSocket => {
            return Err(TransformError::unsupported(
                "compact",
                "the Responses WebSocket dialect cannot host compaction",
            ));
        }
    })
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    if call.client.dialect != Dialect::OpenAi {
        return Err(TransformError::unsupported(
            "compact",
            format!(
                "compaction requests are Responses-shaped; {:?} clients are not converted",
                call.client.dialect
            ),
        ));
    }
    let limits: CodecLimits = call.limits;
    let input: ClientCompactRequestBody = decode_json(call.body(), limits).map_err(codec)?;
    // Native compaction replaces the entire submitted history, so nothing is
    // retained verbatim: the prefix is everything the client sent.
    let retain_from = input.input.len();
    let key = OperationKey {
        operation: Operation::GenerateContent,
        dialect: call.target,
    };
    // Boxed: the adapter's state machine is large and the attempt loop polls
    // this family alongside every other on one stack frame.
    let result = Box::pin(compact(
        call.upstream,
        &key,
        input,
        dialect(call.target)?,
        call.model()?,
        SUMMARY_MAX_TOKENS,
        retain_from,
        CompactLimits {
            max_bytes: limits.max_body_bytes,
            codec: limits,
        },
    ))
    .await;
    let body = match result {
        Ok(body) => body,
        Err(CompactError {
            failure: CompactFailure::Rejected(response),
            ..
        }) => return Ok(Converted::Rejected(*response)),
        Err(CompactError {
            failure: CompactFailure::Transform(error),
            ..
        }) => return Err(error),
    };
    let bytes = encode_json(&body, limits).map_err(codec)?;
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(Converted::Success(WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(bytes),
    }))
}
