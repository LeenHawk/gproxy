use crate::{
    HttpBody, WireRequest, WireResponse,
    capability::Upstream,
    codec::{self, CodecError, CodecErrorKind, CodecLimits},
    transform::{TransformError, TransformErrorKind},
    wire::DeclaredFields,
};
use serde::{Serialize, de::DeserializeOwned};

/// HTTP errors retain their original body and metadata for the operation's
/// concrete error converter. A non-2xx response is never a transport error.
#[derive(Debug)]
pub enum JsonInvocation<T> {
    Success(WireResponse<T>),
    Rejected(WireResponse<HttpBody>),
}

/// Send exactly one already-prepared, typed target request. This helper neither
/// routes nor retries. Dropping its future cancels further adapter work; any
/// upstream side effect remains subject to that operation's recovery contract.
///
/// Only declared fields are encoded/returned. JSON Schema, arguments and other
/// formal JSON values remain intact. The host enforces deadlines and stream idle
/// limits, and replaces authentication according to [`Upstream::send`].
pub async fn invoke_json<U, I, O>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<I>,
    limits: CodecLimits,
) -> Result<JsonInvocation<O>, TransformError>
where
    U: Upstream,
    I: Serialize + DeclaredFields,
    O: DeserializeOwned + DeclaredFields,
{
    let WireRequest {
        method,
        path,
        query,
        mut headers,
        body,
    } = request;
    validate_path(&path)?;
    let host_limits = upstream.limits();
    let write_limits = bound_limits(limits, host_limits.write_bytes);
    let body = codec::encode_json(&body.into_declared(), write_limits)
        .map_err(|error| codec_error(error, false))?;
    // The target JSON bytes replace any former representation of this request.
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    let response = upstream
        .send(
            target,
            WireRequest {
                method,
                path,
                query,
                headers,
                body: HttpBody::Bytes(body),
            },
        )
        .await?;
    receive_json(response, limits, host_limits.read_bytes).await
}

/// Send one request with an empty HTTP body and decode its JSON success body.
/// This is suitable for models/files GET operations; `()` is not encoded as
/// JSON `null`. Non-2xx bodies are returned untouched, as with [`invoke_json`].
pub async fn invoke_empty<U, O>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    limits: CodecLimits,
) -> Result<JsonInvocation<O>, TransformError>
where
    U: Upstream,
    O: DeserializeOwned + DeclaredFields,
{
    validate_path(&request.path)?;
    let WireRequest {
        method,
        path,
        query,
        mut headers,
        body: (),
    } = request;
    for name in [
        http::header::CONTENT_TYPE,
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    let response = upstream
        .send(
            target,
            WireRequest {
                method,
                path,
                query,
                headers,
                body: HttpBody::Bytes(bytes::Bytes::new()),
            },
        )
        .await?;
    receive_json(response, limits, upstream.limits().read_bytes).await
}

async fn receive_json<O: DeserializeOwned + DeclaredFields>(
    response: WireResponse<HttpBody>,
    limits: CodecLimits,
    read_bytes: u64,
) -> Result<JsonInvocation<O>, TransformError> {
    if !response.status.is_success() {
        return Ok(JsonInvocation::Rejected(response));
    }
    let WireResponse {
        status,
        mut headers,
        body,
    } = response;
    let limits = bound_limits(limits, read_bytes);
    let bytes = codec::read_http_body(body, limits)
        .await
        .map_err(|error| codec_error(error, true))?;
    let body: O = codec::decode_json(&bytes, limits).map_err(|error| codec_error(error, true))?;
    // The returned typed body has been rebuilt, so upstream representation
    // lengths/encodings cannot describe a later serialization of that value.
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    Ok(JsonInvocation::Success(WireResponse {
        status,
        headers,
        body: body.into_declared(),
    }))
}

fn validate_path(path: &str) -> Result<(), TransformError> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\', '\r', '\n'])
    {
        return Err(TransformError::shape(
            "request.path",
            "expected an origin-relative path with a separate query",
        ));
    }
    Ok(())
}

fn bound_limits(mut limits: CodecLimits, host_bytes: u64) -> CodecLimits {
    limits.max_body_bytes = limits.max_body_bytes.min(host_bytes);
    limits.max_buffer_bytes = limits.max_buffer_bytes.min(host_bytes);
    limits.max_value_bytes = limits.max_value_bytes.min(host_bytes);
    limits
}

fn codec_error(error: CodecError, response: bool) -> TransformError {
    let kind = match error.kind() {
        CodecErrorKind::Limit => TransformErrorKind::Limit,
        CodecErrorKind::Transport => TransformErrorKind::Host,
        CodecErrorKind::Invalid
        | CodecErrorKind::UnexpectedEof
        | CodecErrorKind::Utf8
        | CodecErrorKind::Json
        | CodecErrorKind::Multipart => {
            if response {
                TransformErrorKind::InvalidResult
            } else {
                TransformErrorKind::InvalidInput
            }
        }
    };
    let detail = error.to_string();
    TransformError::with_source(
        kind,
        if response {
            "upstream.response"
        } else {
            "upstream.request"
        },
        detail,
        error,
    )
}
