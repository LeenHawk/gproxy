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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alpn {
    Http1,
    Http2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    Tls12,
    Tls13,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Http2Setting {
    EnablePush,
    InitialWindowSize,
    MaxFrameSize,
    MaxHeaderListSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoHeader {
    Method,
    Scheme,
    Authority,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Http2Profile {
    pub enable_push: bool,
    pub initial_window_size: u32,
    pub initial_connection_window_size: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: u32,
    pub pseudo_header_order: &'static [PseudoHeader],
    pub settings_order: &'static [Http2Setting],
}

/// Channel-declared native client fingerprint. Edge hosts ignore it because
/// their runtimes own the TLS stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientProfile {
    pub alpn: &'static [Alpn],
    pub min_tls_version: TlsVersion,
    pub max_tls_version: TlsVersion,
    pub cipher_list: &'static str,
    pub curves_list: &'static str,
    pub sigalgs_list: Option<&'static str>,
    pub grease: bool,
    pub http2: Option<Http2Profile>,
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
