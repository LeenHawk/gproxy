use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_channel_api::TransportError;

use crate::boundary::ByteStream;

pub(crate) struct BodyFailure {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
    pub error: TransportError,
}

pub(crate) async fn collect(
    response: http::Response<ByteStream>,
) -> Result<http::Response<Bytes>, BodyFailure> {
    let (parts, mut stream) = response.into_parts();
    let mut body = BytesMut::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => body.extend_from_slice(&chunk),
            Err(error) => {
                return Err(BodyFailure {
                    status: parts.status,
                    headers: parts.headers,
                    body: body.freeze(),
                    error,
                });
            }
        }
    }
    Ok(http::Response::from_parts(parts, body.freeze()))
}
