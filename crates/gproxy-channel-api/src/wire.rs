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
