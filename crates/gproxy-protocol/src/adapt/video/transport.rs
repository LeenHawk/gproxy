use super::*;
use crate::{HttpBody, WireResponse, transform::TransformErrorKind};

/// Read the complete raw response into caller-owned progress before parsing it.
/// Transport/body failure leaves `send_started` set: upstream creation remains
/// uncertain and the durable reservation still prevents another create.
pub(super) async fn invoke<U: Upstream, O: serde::de::DeserializeOwned + DeclaredFields + Clone>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<HttpBody>,
    send_started: &mut bool,
    raw_response: &mut Option<WireResponse<bytes::Bytes>>,
    native_response: &mut Option<WireResponse<O>>,
    limits: VideoLimits,
) -> Result<JsonInvocation<O>, TransformError> {
    *send_started = true;
    let response = upstream.send(target, request).await?;
    if !response.status.is_success() {
        return Ok(JsonInvocation::Rejected(response));
    }
    let mut codec = limits.codec;
    codec.max_body_bytes = codec.max_body_bytes.min(upstream.limits().read_bytes);
    codec.max_buffer_bytes = codec.max_buffer_bytes.min(codec.max_body_bytes);
    let bytes = crate::codec::read_http_body(response.body, codec)
        .await
        .map_err(|e| {
            TransformError::new(
                if e.kind() == crate::codec::CodecErrorKind::Limit {
                    TransformErrorKind::Limit
                } else {
                    TransformErrorKind::Host
                },
                "video.response",
                e.to_string(),
            )
        })?;
    *raw_response = Some(WireResponse {
        status: response.status,
        headers: response.headers.clone(),
        body: bytes.clone(),
    });
    let native: O = crate::codec::decode_json(&bytes, codec)
        .map_err(|e| TransformError::invalid_result("video.response", e.to_string()))?;
    let mut headers = response.headers;
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
    let response = WireResponse {
        status: response.status,
        headers,
        body: native.into_declared(),
    };
    *native_response = Some(WireResponse {
        status: response.status,
        headers: response.headers.clone(),
        body: response.body.clone(),
    });
    Ok(JsonInvocation::Success(response))
}
