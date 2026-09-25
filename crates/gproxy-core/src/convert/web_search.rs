//! Standalone Codex search uses a search-only generation. This also supports
//! providers that cannot combine their hosted search with client functions.

use super::{Call, ClientRequest, Converted, generate};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    codec::{decode_json, encode_json, read_http_body},
    transform::{TransformError, TransformErrorKind, web_search},
};
use gproxy_seaorm::BatchConnectionTrait;

fn codec(error: gproxy_protocol::codec::CodecError) -> TransformError {
    TransformError::with_source(
        TransformErrorKind::InvalidResult,
        "web_search.body",
        error.to_string(),
        error,
    )
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let request = web_search::to_generation(
        generate::decode(call.body(), call.limits)?,
        call.model()?.to_owned(),
    )?;
    let body = encode_json(&request, call.limits).map_err(codec)?;
    let generation = Call {
        core: call.core,
        upstream: call.upstream,
        client: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAi,
        },
        target: call.target,
        model: call.model,
        request: ClientRequest {
            path: "/v1/responses",
            query: None,
            headers: call.request.headers,
            body: &body,
        },
        limits: call.limits,
        state_store: call.state_store,
        state_scope: call.state_scope,
        conversation_key: call.conversation_key,
        provider_id: call.provider_id,
        now_ms: call.now_ms,
        synthesize: false,
        collect: false,
    };
    match Box::pin(generate::buffered(&generation)).await? {
        Converted::Success(response) => {
            let bytes = read_http_body(response.body, call.limits)
                .await
                .map_err(codec)?;
            let output =
                web_search::from_generation(decode_json(&bytes, call.limits).map_err(codec)?)?;
            let mut headers = response.headers;
            headers.remove(http::header::CONTENT_LENGTH);
            headers.remove(http::header::CONTENT_ENCODING);
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
            Ok(Converted::Success(WireResponse {
                status: response.status,
                headers,
                body: HttpBody::Bytes(encode_json(&output, call.limits).map_err(codec)?),
            }))
        }
        rejected @ Converted::Rejected(_) => Ok(rejected),
        Converted::Stream(_) => Err(TransformError::invalid_result(
            "web_search",
            "expected complete search response",
        )),
    }
}
