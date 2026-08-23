use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_channel_api::{BoxFuture, ChannelError};
use gproxy_core::UpstreamTransport;

pub(crate) fn send<'a>(
    transport: &'a dyn UpstreamTransport,
    request: http::Request<Bytes>,
) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
    let send = transport.send(request);
    Box::pin(async move {
        let response = send
            .await
            .map_err(|error| ChannelError::Refresh(error.to_string()))?;
        let (parts, mut stream) = response.into_parts();
        let mut body = BytesMut::new();
        while let Some(chunk) = stream.next().await {
            body.extend_from_slice(
                &chunk.map_err(|error| ChannelError::Refresh(error.to_string()))?,
            );
        }
        Ok(http::Response::from_parts(parts, body.freeze()))
    })
}
