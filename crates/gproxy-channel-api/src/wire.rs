//! Wire-adjacent primitives shared by the contract and the engine.

use bytes::Bytes;

/// Failures crossing the wire to an upstream.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connect: {0}")]
    Connect(String),
    #[error("timed out")]
    Timeout,
    #[error("stream interrupted: {0}")]
    Interrupted(String),
}

/// Native transport fingerprint selected by a channel. Edge hosts ignore it;
/// they do not control the runtime TLS stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProfile {
    ClaudeCode,
}

/// Response body stream. Zero-copy passthrough is the default path: frames
/// flow as refcounted `Bytes` and are only re-encoded when something must
/// rewrite them.
///
/// The `Send` split is the one language-level tax carried for the wasm
/// target (single-threaded executors; futures there are not `Send`).
#[cfg(not(target_arch = "wasm32"))]
pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, TransportError>> + Send>>;
#[cfg(target_arch = "wasm32")]
pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, TransportError>>>>;

/// Stable credential identity. i64 to match relational primary keys;
/// embedders without a database can hand out any distinct values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CredentialId(pub i64);

/// `Send` on native, nothing on wasm — the marker that lets one trait
/// definition serve both targets instead of duplicating it under `#[cfg]`.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

/// `Sync` on native, unconstrained on single-threaded wasm. The core's
/// returned stream owns shared host services across settlement awaits.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync + ?Sized> MaybeSync for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSync for T {}

/// One websocket frame, transport-agnostic. Hosts bridge these to their
/// native socket type; channels and the engine never see axum or a
/// platform socket.
#[derive(Debug)]
pub enum WsFrame {
    Text(String),
    Binary(Bytes),
    Close(Option<u16>),
}

/// A connected websocket, either direction. `recv` returning `None`
/// means the peer closed cleanly.
pub trait WsDuplex: MaybeSend {
    fn send<'a>(&'a mut self, frame: WsFrame) -> crate::BoxFuture<'a, Result<(), TransportError>>;
    fn recv<'a>(&'a mut self) -> crate::BoxFuture<'a, Result<Option<WsFrame>, TransportError>>;
}
