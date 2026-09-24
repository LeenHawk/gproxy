//! Shared bounded service I/O and channel errors.

use crate::OutboundClient;
use crate::channel::ChannelError;
use futures_util::StreamExt;
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{HttpBody, WireResponse};
use http::{HeaderMap, Method, StatusCode};


pub(super) fn invalid_config(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
}

pub(super) fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| invalid_response(e.to_string()))?;
                out.extend_from_slice(&chunk);
            }
            Ok(Bytes::from(out))
        }
    }
}

pub(super) async fn send_json(
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
        .map_err(|error| invalid_config(error.to_string()))?;
    let WireResponse {
        status,
        headers,
        body,
    } = client.send(request).await?;
    Ok((status, headers, read_body(body).await?))
}
