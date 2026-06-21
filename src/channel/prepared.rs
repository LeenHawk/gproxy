//! The output of [`Channel::prepare`](crate::channel::Channel::prepare).

use bytes::Bytes;

/// A fully-addressed, auth-injected upstream request ready for
/// [`UpstreamClient::send`](crate::http::client::UpstreamClient::send).
///
/// Proxy and TLS-emulation are NOT carried here — they are per-credential /
/// global / channel-default concerns resolved by the executor
/// (see [`crate::channel::resolve`]), not the channel's to decide.
#[derive(Debug)]
pub struct PreparedRequest {
    /// `request.uri()` MUST be absolute (scheme + authority + path + query) —
    /// wreq cannot route a relative URI (see [`http_util::join_url`]).
    ///
    /// [`http_util::join_url`]: crate::channel::http_util::join_url
    pub request: http::Request<Bytes>,
}

impl PreparedRequest {
    /// Wrap a built request.
    pub fn new(request: http::Request<Bytes>) -> Self {
        Self { request }
    }

    /// Consume into the bare `http::Request` for the transport.
    pub fn into_http(self) -> http::Request<Bytes> {
        self.request
    }
}
