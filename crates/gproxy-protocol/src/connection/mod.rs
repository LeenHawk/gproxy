//! HTTP messages, body representations, and independent WebSocket connections.
//!
//! HTTP metadata stays outside the body even when the body has been decoded
//! into a vendor's JSON type. Multipart and event streams are HTTP bodies.
//! A WebSocket handshake uses HTTP; the established socket has its own duplex
//! interface and is never an HTTP body variant.

mod body;
mod websocket;

pub use body::{ByteStream, HttpBody, Multipart, MultipartPart, PartStream, StreamFraming};
pub use websocket::{WebSocket, WsClose, WsFrame, WsReceiver, WsSender};

pub use bytes::Bytes;
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};

/// An error supplied by the transport adapter while receiving or sending data.
pub type TransportError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// An HTTP request with five elements.
///
/// `path` and `query` retain their original percent encoding. `query` excludes
/// the leading `?`; `None` and `Some("")` distinguish an absent and empty query.
/// Repeated parameters and their order are preserved. The transport adapter
/// supplies the destination host independently.
///
/// `B` can be raw [`HttpBody`], a decoded JSON body, or parsed [`Multipart`].
/// GET operations use `()` for an absent body, not a JSON `null` body.
#[derive(Debug)]
pub struct WireRequest<B = HttpBody> {
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: B,
}

/// An HTTP response with three elements, independent of the body representation.
#[derive(Debug)]
pub struct WireResponse<B = HttpBody> {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: B,
}
