//! The transport contract: an already prepared absolute request goes out, a
//! complete response or an upgraded socket comes back. Implementations never
//! choose credentials, convert protocols or retry a consumed body.

use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        UpstreamConnection,
    },
};

/// Native clients must be shareable; WASM transports may own local JS handles.
#[cfg(not(target_arch = "wasm32"))]
pub trait ClientBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> ClientBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait ClientBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> ClientBounds for T {}

/// A host-selected HTTP client with optional WebSocket support.
///
/// Non-2xx responses remain complete responses. Implementations preserve
/// streaming bodies and enforce host-configured deadlines and transfer limits.
/// They must not implicitly retry a consumed body or forward authentication to a
/// different origin on redirect. Body-transfer errors stay on the returned body
/// stream; only failures before any response head arrives are returned here.
pub trait OutboundClient: ClientBounds {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>>;

    /// Optional duplex transport. A rejected handshake is `Ok(Rejected)` with
    /// its full HTTP response; transport failures are errors.
    fn connect<'a>(
        &'a self,
        _request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                "this client does not support WebSocket connections",
            ))
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::Client;
    #[cfg(any(feature = "reqwest", feature = "wreq"))]
    use futures_util::{SinkExt, StreamExt};
    #[cfg(any(feature = "reqwest", feature = "wreq"))]
    use gproxy_protocol::connection::{TransportError, WebSocket, WsClose, WsFrame};
    #[cfg(any(feature = "reqwest", feature = "wreq"))]
    use http::{HeaderMap, HeaderValue, StatusCode, header};

    #[cfg(any(feature = "reqwest", feature = "wreq"))]
    fn start_error(source: impl std::error::Error + Send + Sync + 'static) -> CapabilityError {
        CapabilityError::with_source(
            CapabilityErrorKind::Transport,
            CapabilityErrorStage::Start,
            "upstream request failed before a response arrived",
            source,
        )
    }
    #[cfg(any(feature = "reqwest", feature = "wreq"))]
    fn boxed(source: impl std::error::Error + Send + Sync + 'static) -> TransportError {
        Box::new(source)
    }

    /// The upgrade layers own the handshake headers; a caller-supplied copy would
    /// be duplicated or contradict the generated key.
    #[cfg(any(feature = "reqwest", feature = "wreq"))]
    fn split_handshake_headers(headers: &mut HeaderMap) -> Vec<String> {
        let protocols = headers
            .get_all(header::SEC_WEBSOCKET_PROTOCOL)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect();
        for name in [
            header::UPGRADE,
            header::CONNECTION,
            header::SEC_WEBSOCKET_KEY,
            header::SEC_WEBSOCKET_VERSION,
            header::SEC_WEBSOCKET_EXTENSIONS,
            header::SEC_WEBSOCKET_PROTOCOL,
        ] {
            headers.remove(name);
        }
        protocols
    }

    #[cfg(any(feature = "reqwest", feature = "wreq"))]
    fn handshake(status: StatusCode, headers: &HeaderMap<HeaderValue>) -> WireResponse<()> {
        WireResponse {
            status,
            headers: headers.clone(),
            body: (),
        }
    }

    #[cfg(not(any(feature = "reqwest", feature = "wreq")))]
    fn no_backend() -> CapabilityError {
        CapabilityError::new(
            CapabilityErrorKind::Unsupported,
            CapabilityErrorStage::Start,
            "gproxy-client was built without a transport backend feature",
        )
    }

    impl OutboundClient for Client {
        fn send<'a>(
            &'a self,
            request: http::Request<HttpBody>,
        ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
            Box::pin(async move {
                #[cfg(any(feature = "reqwest", feature = "wreq"))]
                {
                    let (parts, body) = request.into_parts();
                    match self {
                        #[cfg(feature = "reqwest")]
                        Client::Reqwest(client) => reqwest_send(client, parts, body).await,
                        #[cfg(feature = "wreq")]
                        Client::Wreq(client) => wreq_send(client, parts, body).await,
                        #[cfg(feature = "reqwest-native")]
                        Client::ReqwestNative(client) => {
                            reqwest_native_send(client, parts, body).await
                        }
                    }
                }
                #[cfg(not(any(feature = "reqwest", feature = "wreq")))]
                {
                    let _ = request;
                    Err(no_backend())
                }
            })
        }

        fn connect<'a>(
            &'a self,
            request: http::Request<()>,
        ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
            Box::pin(async move {
                #[cfg(any(feature = "reqwest", feature = "wreq"))]
                {
                    let (parts, ()) = request.into_parts();
                    match self {
                        #[cfg(feature = "reqwest")]
                        Client::Reqwest(client) => reqwest_connect(client, parts).await,
                        #[cfg(feature = "wreq")]
                        Client::Wreq(client) => wreq_connect(client, parts).await,
                        // The pool hands out the rustls reqwest client for
                        // WebSocket profiles of this backend; a direct call
                        // on the HTTP client has no upgrade layer.
                        #[cfg(feature = "reqwest-native")]
                        Client::ReqwestNative(_) => {
                            let _ = parts;
                            Err(CapabilityError::new(
                                CapabilityErrorKind::Unsupported,
                                CapabilityErrorStage::Start,
                                "the reqwest_native client does not upgrade to WebSocket; use the pool's WebSocket client",
                            ))
                        }
                    }
                }
                #[cfg(not(any(feature = "reqwest", feature = "wreq")))]
                {
                    let _ = request;
                    Err(no_backend())
                }
            })
        }
    }

    #[cfg(feature = "reqwest")]
    async fn reqwest_send(
        client: &reqwest::Client,
        parts: http::request::Parts,
        body: HttpBody,
    ) -> Result<WireResponse<HttpBody>, CapabilityError> {
        let body = match body {
            HttpBody::Bytes(bytes) => reqwest::Body::from(bytes),
            HttpBody::Stream(stream) => reqwest::Body::wrap_stream(stream),
        };
        let response = client
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(start_error)?;
        Ok(WireResponse {
            status: response.status(),
            headers: response.headers().clone(),
            body: HttpBody::Stream(Box::pin(
                response.bytes_stream().map(|chunk| chunk.map_err(boxed)),
            )),
        })
    }

    #[cfg(feature = "reqwest")]
    async fn reqwest_connect(
        client: &reqwest::Client,
        mut parts: http::request::Parts,
    ) -> Result<UpstreamConnection, CapabilityError> {
        use reqwest_websocket::{CloseCode, Message, Upgrade};
        let protocols = split_handshake_headers(&mut parts.headers);
        let response = client
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .upgrade()
            .protocols(protocols)
            .send()
            .await
            .map_err(start_error)?;
        if response.status() != StatusCode::SWITCHING_PROTOCOLS {
            let response = response.into_inner();
            return Ok(UpstreamConnection::Rejected(WireResponse {
                status: response.status(),
                headers: response.headers().clone(),
                body: HttpBody::Stream(Box::pin(
                    response.bytes_stream().map(|chunk| chunk.map_err(boxed)),
                )),
            }));
        }
        let head = handshake(response.status(), response.headers());
        let socket = response.into_websocket().await.map_err(start_error)?;
        let (sink, stream) = socket.split();
        let incoming = stream.map(|message| {
            message.map_err(boxed).map(|message| match message {
                Message::Text(text) => WsFrame::Text(text),
                Message::Binary(bytes) => WsFrame::Binary(bytes),
                Message::Ping(bytes) => WsFrame::Ping(bytes),
                Message::Pong(bytes) => WsFrame::Pong(bytes),
                Message::Close { code, reason } => WsFrame::Close(Some(WsClose {
                    code: u16::from(code),
                    reason,
                })),
            })
        });
        let outgoing = sink.with(|frame: WsFrame| async move {
            Ok::<_, TransportError>(match frame {
                WsFrame::Text(text) => Message::Text(text),
                WsFrame::Binary(bytes) => Message::Binary(bytes),
                WsFrame::Ping(bytes) => Message::Ping(bytes),
                WsFrame::Pong(bytes) => Message::Pong(bytes),
                WsFrame::Close(Some(close)) => Message::Close {
                    code: CloseCode::from(close.code),
                    reason: close.reason,
                },
                WsFrame::Close(None) => Message::Close {
                    code: CloseCode::Normal,
                    reason: String::new(),
                },
            })
        });
        Ok(UpstreamConnection::Connected {
            handshake: head,
            socket: WebSocket {
                incoming: Box::pin(incoming),
                outgoing: Box::pin(outgoing),
            },
        })
    }

    #[cfg(feature = "reqwest-native")]
    async fn reqwest_native_send(
        client: &reqwest_native::Client,
        parts: http::request::Parts,
        body: HttpBody,
    ) -> Result<WireResponse<HttpBody>, CapabilityError> {
        let body = match body {
            HttpBody::Bytes(bytes) => reqwest_native::Body::from(bytes),
            HttpBody::Stream(stream) => reqwest_native::Body::wrap_stream(stream),
        };
        let response = client
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(start_error)?;
        Ok(WireResponse {
            status: response.status(),
            headers: response.headers().clone(),
            body: HttpBody::Stream(Box::pin(
                response.bytes_stream().map(|chunk| chunk.map_err(boxed)),
            )),
        })
    }

    #[cfg(feature = "wreq")]
    async fn wreq_send(
        client: &wreq::Client,
        parts: http::request::Parts,
        body: HttpBody,
    ) -> Result<WireResponse<HttpBody>, CapabilityError> {
        let body = match body {
            HttpBody::Bytes(bytes) => wreq::Body::from(bytes),
            HttpBody::Stream(stream) => wreq::Body::wrap_stream(stream),
        };
        let response = client
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(start_error)?;
        Ok(WireResponse {
            status: response.status(),
            headers: response.headers().clone(),
            body: HttpBody::Stream(Box::pin(
                response.bytes_stream().map(|chunk| chunk.map_err(boxed)),
            )),
        })
    }

    #[cfg(feature = "wreq")]
    async fn wreq_connect(
        client: &wreq::Client,
        mut parts: http::request::Parts,
    ) -> Result<UpstreamConnection, CapabilityError> {
        use wreq::ws::message::{CloseCode, CloseFrame, Message};
        let protocols = split_handshake_headers(&mut parts.headers);
        let response = client
            .websocket(parts.uri.to_string())
            .headers(parts.headers)
            .protocols(protocols)
            .send()
            .await
            .map_err(start_error)?;
        if response.status() != StatusCode::SWITCHING_PROTOCOLS {
            // wreq's WebSocketResponse never releases the underlying Response, so
            // a rejected handshake keeps its status and headers but not its body.
            // Prefer the reqwest backend where the vendor error body matters.
            return Ok(UpstreamConnection::Rejected(WireResponse {
                status: response.status(),
                headers: response.headers().clone(),
                body: HttpBody::Bytes(gproxy_protocol::connection::Bytes::new()),
            }));
        }
        let head = handshake(response.status(), response.headers());
        let socket = response.into_websocket().await.map_err(start_error)?;
        let (sink, stream) = socket.split();
        let incoming = stream.map(|message| {
            message.map_err(boxed).map(|message| match message {
                Message::Text(text) => WsFrame::Text(text.as_str().to_owned()),
                Message::Binary(bytes) => WsFrame::Binary(bytes),
                Message::Ping(bytes) => WsFrame::Ping(bytes),
                Message::Pong(bytes) => WsFrame::Pong(bytes),
                Message::Close(Some(frame)) => WsFrame::Close(Some(WsClose {
                    code: u16::from(frame.code),
                    reason: frame.reason.as_str().to_owned(),
                })),
                Message::Close(None) => WsFrame::Close(None),
            })
        });
        let outgoing = sink.with(|frame: WsFrame| async move {
            Ok::<_, TransportError>(match frame {
                WsFrame::Text(text) => Message::Text(text.into()),
                WsFrame::Binary(bytes) => Message::Binary(bytes),
                WsFrame::Ping(bytes) => Message::Ping(bytes),
                WsFrame::Pong(bytes) => Message::Pong(bytes),
                WsFrame::Close(Some(close)) => Message::Close(Some(CloseFrame {
                    code: CloseCode::from(close.code),
                    reason: close.reason.into(),
                })),
                WsFrame::Close(None) => Message::Close(None),
            })
        });
        Ok(UpstreamConnection::Connected {
            handshake: head,
            socket: WebSocket {
                incoming: Box::pin(incoming),
                outgoing: Box::pin(outgoing),
            },
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::Client;
    #[cfg(feature = "reqwest")]
    use futures_util::StreamExt;
    #[cfg(feature = "reqwest")]
    use gproxy_protocol::connection::{Bytes, TransportError};

    #[cfg(feature = "reqwest")]
    fn unsupported(message: &'static str) -> CapabilityError {
        CapabilityError::new(
            CapabilityErrorKind::Unsupported,
            CapabilityErrorStage::Start,
            message,
        )
    }

    impl OutboundClient for Client {
        fn send<'a>(
            &'a self,
            request: http::Request<HttpBody>,
        ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
            match self {
                #[cfg(feature = "fetch")]
                Client::Fetch(client) => client.send(request),
                #[cfg(feature = "reqwest")]
                Client::Reqwest(client) => Box::pin(reqwest_send(client, request)),
                Client::Host(client) => client.send(request),
            }
        }

        fn connect<'a>(
            &'a self,
            request: http::Request<()>,
        ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
            match self {
                #[cfg(feature = "fetch")]
                Client::Fetch(client) => client.connect(request),
                #[cfg(feature = "reqwest")]
                Client::Reqwest(_) => Box::pin(async {
                    Err(unsupported(
                        "the reqwest Fetch fallback cannot upgrade to WebSocket; enable `fetch`/`workers` or inject a host client",
                    ))
                }),
                Client::Host(client) => client.connect(request),
            }
        }
    }

    /// reqwest on Fetch takes a complete body: a streaming request body is
    /// collected first. Response bodies stream as on native.
    #[cfg(feature = "reqwest")]
    async fn reqwest_send(
        client: &reqwest::Client,
        request: http::Request<HttpBody>,
    ) -> Result<WireResponse<HttpBody>, CapabilityError> {
        let (parts, body) = request.into_parts();
        let bytes = match body {
            HttpBody::Bytes(bytes) => bytes,
            HttpBody::Stream(mut stream) => {
                let mut out = Vec::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| {
                        CapabilityError::new(
                            CapabilityErrorKind::Transport,
                            CapabilityErrorStage::BodyTransfer,
                            e.to_string(),
                        )
                    })?;
                    out.extend_from_slice(&chunk);
                }
                Bytes::from(out)
            }
        };
        let response = client
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(bytes)
            .send()
            .await
            .map_err(|e| {
                CapabilityError::with_source(
                    CapabilityErrorKind::Transport,
                    CapabilityErrorStage::Start,
                    "upstream request failed before a response arrived",
                    e,
                )
            })?;
        Ok(WireResponse {
            status: response.status(),
            headers: response.headers().clone(),
            body: HttpBody::Stream(Box::pin(
                response
                    .bytes_stream()
                    .map(|chunk| chunk.map_err(|e| Box::new(e) as TransportError)),
            )),
        })
    }
}
