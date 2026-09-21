//! Codex Guardian review and classification through ordinary generation on
//! the target dialect. The client speaks the Responses-shaped guardian wire;
//! the verdict comes back as one completed assistant message whose text is
//! the native result (review JSON, or the `high`/`low` label).
//!
//! Codex always asks for a stream. Streamed delivery is not converted yet;
//! a request with `stream: true` is refused explicitly rather than answered
//! in a framing the client did not ask for.

use super::{Call, Converted};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    adapt::guardian::{GuardianClassifyInvocation, GuardianLimits, GuardianReviewInvocation},
    codec::{CodecLimits, decode_json, encode_json},
    openai::guardian::GuardianRequestBody,
    transform::{
        TransformError, TransformErrorKind,
        guardian::{
            GuardianClassifyResult, GuardianOperation, GuardianPreparedRequest,
            GuardianRequestContext, prepare_claude, prepare_gemini, prepare_openai_chat,
            prepare_openai_responses,
        },
    },
    wire::openai::responses as r,
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE};

/// Output budget for the verdict. Review returns a short JSON object and
/// classification a single label; the budget only guards against runaway
/// generation on the target.
const MAX_TOKENS: i64 = 1024;

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

fn fresh_id(prefix: &str) -> String {
    let mut bytes = [0u8; 12];
    let _ = getrandom::fill(&mut bytes);
    let mut id = String::with_capacity(prefix.len() + 1 + bytes.len() * 2);
    id.push_str(prefix);
    id.push('_');
    for byte in bytes {
        id.push_str(&format!("{byte:02x}"));
    }
    id
}

fn prepare(
    input: GuardianRequestBody,
    target: Dialect,
    context: GuardianRequestContext,
) -> Result<GuardianPreparedRequest, TransformError> {
    Ok(match target {
        Dialect::Claude => prepare_claude(input, context)?.value,
        Dialect::Gemini => prepare_gemini(input, context)?.value,
        Dialect::OpenAiChat => prepare_openai_chat(input, context)?.value,
        Dialect::OpenAi => prepare_openai_responses(input, context)?.value,
        Dialect::OpenAiResponsesWebSocket => {
            return Err(TransformError::unsupported(
                "guardian",
                "the Responses WebSocket dialect cannot host guardian generation",
            ));
        }
    })
}

/// The verdict as the text Codex parses from the guardian model's message.
enum Verdict {
    Text(String),
    Rejected(WireResponse<HttpBody>),
}

async fn invoke<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    operation: GuardianOperation,
    prepared: GuardianPreparedRequest,
) -> Result<Verdict, TransformError> {
    let key = OperationKey {
        operation: Operation::GenerateContent,
        dialect: call.target,
    };
    let limits = GuardianLimits {
        codec: call.limits,
        max_request_bytes: call.limits.max_body_bytes,
    };
    // Boxed: the adapters' state machines are large and the attempt loop
    // polls this family alongside every other on one stack frame.
    Ok(match operation {
        GuardianOperation::Review => {
            match Box::pin(gproxy_protocol::adapt::guardian::review(
                call.upstream,
                &key,
                prepared,
                limits,
            ))
            .await?
            {
                GuardianReviewInvocation::Success { result, .. } => {
                    Verdict::Text(serde_json::to_string(&result).map_err(|e| {
                        TransformError::new(
                            TransformErrorKind::InvalidResult,
                            "guardian.review",
                            e.to_string(),
                        )
                    })?)
                }
                GuardianReviewInvocation::Rejected(response) => Verdict::Rejected(response),
                GuardianReviewInvocation::Invalid { error, .. } => return Err(error),
            }
        }
        GuardianOperation::Classify => {
            match Box::pin(gproxy_protocol::adapt::guardian::classify(
                call.upstream,
                &key,
                prepared,
                limits,
            ))
            .await?
            {
                GuardianClassifyInvocation::Success { result, .. } => Verdict::Text(
                    match result {
                        GuardianClassifyResult::High => "high",
                        GuardianClassifyResult::Low => "low",
                    }
                    .to_owned(),
                ),
                GuardianClassifyInvocation::Rejected(response) => Verdict::Rejected(response),
                GuardianClassifyInvocation::Invalid { error, .. } => return Err(error),
            }
        }
    })
}

fn response(
    text: String,
    client_model: String,
    parallel_tool_calls: bool,
    tool_choice: r::input::ToolChoice,
    created_at: i64,
) -> r::GenerateContentResponseBody {
    let message = r::input::ResponseOutputMessage::builder(
        fresh_id("msg"),
        vec![r::input::OutputContent::Text(
            r::input::ResponseOutputText::builder(
                r::input::ResponseOutputTextType::ResponseOutputText,
                text.clone(),
                Vec::new(),
                Vec::new(),
            )
            .build(),
        )],
        r::input::OutputMessageRole::Assistant,
        r::input::OutputMessageStatus::Completed,
        r::input::MessageType::Message,
    )
    .build();
    r::GenerateContentResponseBody::builder(
        fresh_id("resp"),
        created_at,
        None,
        None,
        None,
        None,
        client_model,
        r::ResponseObject::Response,
        vec![r::ResponseOutputItem::Message(message)],
        parallel_tool_calls,
        None,
        tool_choice,
        Vec::new(),
        None,
    )
    .status(r::ResponseStatus::Completed)
    .output_text(text)
    .build()
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let operation = match call.client.operation {
        Operation::GuardianReview => GuardianOperation::Review,
        Operation::GuardianClassify => GuardianOperation::Classify,
        other => {
            return Err(TransformError::unsupported(
                "guardian",
                format!("{other:?} is not a guardian operation"),
            ));
        }
    };
    if call.client.dialect != Dialect::OpenAi {
        return Err(TransformError::unsupported(
            "guardian",
            format!(
                "guardian requests are Responses-shaped; {:?} clients are not converted",
                call.client.dialect
            ),
        ));
    }
    let limits: CodecLimits = call.limits;
    let input: GuardianRequestBody = decode_json(call.body(), limits).map_err(codec)?;
    if input.stream {
        return Err(TransformError::unsupported(
            "guardian.stream",
            "streamed guardian verdicts are not converted yet; send stream=false",
        ));
    }
    let tool_choice: r::input::ToolChoice = serde_json::from_value(serde_json::Value::String(
        input.tool_choice.clone(),
    ))
    .map_err(|e| {
        TransformError::new(
            TransformErrorKind::InvalidInput,
            "tool_choice",
            e.to_string(),
        )
    })?;
    let client_model = input.model.clone();
    let parallel_tool_calls = input.parallel_tool_calls;
    let context = GuardianRequestContext {
        target_model: call.model()?.to_owned(),
        max_tokens: MAX_TOKENS,
        operation,
    };
    let prepared = prepare(input, call.target, context)?;
    let text = match Box::pin(invoke(call, operation, prepared)).await? {
        Verdict::Text(text) => text,
        Verdict::Rejected(response) => return Ok(Converted::Rejected(response)),
    };
    let body = response(
        text,
        client_model,
        parallel_tool_calls,
        tool_choice,
        call.now_ms.div_euclid(1000),
    );
    let bytes = encode_json(&body, limits).map_err(codec)?;
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(Converted::Success(WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(bytes),
    }))
}
