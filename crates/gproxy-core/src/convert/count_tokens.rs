//! Token counting across dialects: one POST to the target's native counting
//! endpoint, request and response mapped by `transform::count_tokens`. Counts
//! come from the selected upstream model's tokenizer; nothing is estimated.
//! OpenAI is never a counting target (see `count_tokens_endpoint`), and the
//! Chat Completions dialect has no counting operation to convert from.

use super::{Call, Converted};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    adapt::{JsonInvocation, invoke_json},
    codec::{CodecLimits, decode_json, encode_json},
    transform::{
        TransformError, TransformErrorKind, count_tokens as ct,
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{claude::count_tokens as c, gemini::count_tokens as g, openai::count_tokens as o},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::Method;
use serde::{Serialize, de::DeserializeOwned};

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

fn decode<T: DeserializeOwned>(body: &[u8], limits: CodecLimits) -> Result<T, TransformError> {
    decode_json(body, limits).map_err(codec)
}

fn flow() -> IdentityFlow {
    let mut bytes = [0u8; 16];
    crate::ids::fill_random(&mut bytes);
    IdentityFlow::new(IdNamespace(bytes))
}

fn success<T: Serialize>(
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: &T,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    let body = encode_json(body, limits).map_err(codec)?;
    Ok(Converted::Success(WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(body),
    }))
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let target = call.target;
    let client = call.client.dialect;
    if matches!(target, Dialect::OpenAi | Dialect::OpenAiChat) || client == Dialect::OpenAiChat {
        return Err(TransformError::unsupported(
            "count_tokens",
            format!("no conversion from {client:?} to {target:?}"),
        ));
    }
    let model = call.model()?;
    let endpoint = super::endpoints::count_tokens_endpoint(target, model)?;
    let key = OperationKey {
        operation: Operation::CountTokens,
        dialect: target,
    };
    let limits = call.limits;
    let upstream = call.upstream;
    let policy = TargetIdPolicy::new(target);
    macro_rules! post {
        ($body:expr, $native:ty) => {{
            let request = WireRequest {
                method: Method::POST,
                path: endpoint.path.clone(),
                query: endpoint.query.clone(),
                headers: call.request.headers.clone(),
                body: $body,
            };
            match invoke_json::<_, _, $native>(upstream, &key, request, limits).await? {
                JsonInvocation::Rejected(response) => return Ok(Converted::Rejected(response)),
                JsonInvocation::Success(response) => response,
            }
        }};
    }
    match (client, target) {
        (Dialect::Claude, Dialect::Gemini) => {
            let input: c::CountTokensRequestBody = decode(call.body(), limits)?;
            let context = ct::ClaudeCountContext::for_request(&input);
            let mapped =
                ct::claude_to_gemini(input, model, Default::default(), &mut flow(), &policy)?;
            let native = post!(mapped.value, g::CountTokensResponseBody);
            let mapped = ct::gemini_to_claude_response(native.body, context)?;
            success(native.status, native.headers, &mapped.value, limits)
        }
        (Dialect::Gemini, Dialect::Claude) => {
            let input: g::CountTokensRequestBody = decode(call.body(), limits)?;
            let mapped = ct::gemini_to_claude(input, model, &mut flow(), &policy)?;
            let native = post!(mapped.value, c::CountTokensResponseBody);
            let mapped = ct::claude_to_gemini_response(native.body)?;
            success(native.status, native.headers, &mapped.value, limits)
        }
        (Dialect::OpenAi, Dialect::Claude) => {
            let input: o::CountTokensRequestBody = decode(call.body(), limits)?;
            let mapped = ct::openai_to_claude(input, model, Default::default())?;
            let native = post!(mapped.value, c::CountTokensResponseBody);
            let mapped = ct::claude_to_openai_response(native.body)?;
            success(native.status, native.headers, &mapped.value, limits)
        }
        (Dialect::OpenAi, Dialect::Gemini) => {
            let input: o::CountTokensRequestBody = decode(call.body(), limits)?;
            let mapped = ct::openai_to_gemini(input, model, Default::default())?;
            let native = post!(mapped.value, g::CountTokensResponseBody);
            let mapped = ct::gemini_to_openai_response(native.body)?;
            success(native.status, native.headers, &mapped.value, limits)
        }
        (client, target) => Err(TransformError::unsupported(
            "count_tokens",
            format!("no conversion from {client:?} to {target:?}"),
        )),
    }
}
