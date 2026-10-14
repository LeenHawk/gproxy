//! The calls a channel makes outside the operation path — quota probes,
//! token refresh, device login — and the money values they read.
//!
//! These exchanges are fully buffered and their failures are errors rather
//! than responses: a quota probe that answers 401 is a `ChannelError`, not a
//! snapshot with a 401 in it.

use super::http::read_body;
use crate::OutboundClient;
use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{HttpBody, WireResponse};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;

/// One bounded exchange. `body` is sent verbatim; `None` is an empty body,
/// which is what a GET wants.
pub(crate) async fn send(
    client: &dyn OutboundClient,
    method: Method,
    url: &str,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
) -> Result<(StatusCode, HeaderMap, Bytes), ChannelError> {
    let mut builder = http::Request::builder().method(method).uri(url);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    let request = builder
        .body(match body {
            Some(bytes) => HttpBody::Bytes(Bytes::from(bytes)),
            None => HttpBody::Bytes(Bytes::new()),
        })
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    let WireResponse {
        status,
        headers,
        body,
    } = client.send(request).await?;
    Ok((status, headers, read_body(body).await?))
}

/// `Accept: application/json` plus a bearer token. Only the channels whose
/// ability calls carry nothing else build their headers this way.
#[cfg(any(
    feature = "deepseek",
    feature = "glm",
    feature = "minimax",
    feature = "kimi",
    feature = "opencode",
    feature = "openrouter",
    feature = "vercel",
    feature = "xai"
))]
pub(crate) fn bearer(token: &str) -> Result<HeaderMap, ChannelError> {
    use http::{HeaderValue, header};
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    Ok(headers)
}

/// The status is a definite upstream refusal of this ability call.
pub(crate) fn require_success(status: StatusCode, body: Bytes) -> Result<Bytes, ChannelError> {
    if status.is_success() {
        Ok(body)
    } else {
        Err(ChannelError::UpstreamResponse { status, body })
    }
}

/// A number or a numeric string, as vendors report money either way.
pub(crate) fn decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => number.to_string().parse().ok(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
