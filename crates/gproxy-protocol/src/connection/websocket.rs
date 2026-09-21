use std::{fmt, pin::Pin};

use futures_core::Stream;
use futures_sink::Sink;

use super::{Bytes, TransportError};

/// A complete WebSocket data message or control frame. The transport adapter
/// handles wire fragmentation, masking, and protocol validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsFrame {
    Text(String),
    Binary(Bytes),
    Ping(Bytes),
    Pong(Bytes),
    Close(Option<WsClose>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsClose {
    pub code: u16,
    pub reason: String,
}

#[cfg(not(target_arch = "wasm32"))]
pub type WsReceiver = Pin<Box<dyn Stream<Item = Result<WsFrame, TransportError>> + Send + 'static>>;

#[cfg(target_arch = "wasm32")]
pub type WsReceiver = Pin<Box<dyn Stream<Item = Result<WsFrame, TransportError>> + 'static>>;

#[cfg(not(target_arch = "wasm32"))]
pub type WsSender = Pin<Box<dyn Sink<WsFrame, Error = TransportError> + Send + 'static>>;

#[cfg(target_arch = "wasm32")]
pub type WsSender = Pin<Box<dyn Sink<WsFrame, Error = TransportError> + 'static>>;

/// An established connection. Sending and receiving are independent; there is
/// no request/response pairing. The HTTP handshake is represented separately
/// by [`super::WireRequest`] and [`super::WireResponse`].
pub struct WebSocket {
    pub incoming: WsReceiver,
    pub outgoing: WsSender,
}

impl fmt::Debug for WebSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebSocket { incoming: .., outgoing: .. }")
    }
}
