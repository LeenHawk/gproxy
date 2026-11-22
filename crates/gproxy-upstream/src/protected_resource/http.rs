use gproxy_client::OutboundClient;
use gproxy_protocol::{
    HttpBody, WireResponse,
    codec::{CodecLimits, read_http_body},
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::Value;

use crate::UpstreamError;

/// Metadata documents and token replies are small; a megabyte is generous.
const LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 1 << 20,
    max_value_bytes: 1 << 20,
    max_body_bytes: 1 << 20,
    max_line_bytes: 1 << 20,
    max_part_bytes: 1 << 20,
    max_parts: 1,
};

pub(super) async fn send(
    client: &dyn OutboundClient,
    method: Method,
    url: &str,
    mut headers: HeaderMap,
    body: Option<Vec<u8>>,
) -> Result<(StatusCode, HeaderMap, Bytes), UpstreamError> {
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    let mut builder = http::Request::builder().method(method).uri(url);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    let request = builder
        .body(HttpBody::Bytes(Bytes::from(body.unwrap_or_default())))
        .map_err(|error| UpstreamError::InvalidConfig(error.to_string()))?;
    let WireResponse {
        status,
        headers,
        body,
    } = client.send(request).await?;
    let body = read_http_body(body, LIMITS)
        .await
        .map_err(|error| UpstreamError::InvalidResponse(error.to_string()))?;
    Ok((status, headers, body))
}

/// A JSON document, or `None` when the server has none at this URL.
pub(super) async fn get_json(
    client: &dyn OutboundClient,
    url: &str,
) -> Result<Option<Value>, UpstreamError> {
    let (status, _, body) = send(client, Method::GET, url, HeaderMap::new(), None).await?;
    if !status.is_success() {
        return Ok(None);
    }
    Ok(serde_json::from_slice(&body).ok())
}
