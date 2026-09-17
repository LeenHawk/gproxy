//! Public operation and execution-data lifecycle API. Operation wrappers validate
//! their identity and share HTTP/WS dispatch; actual execution, Store assembly
//! and account capabilities still return NotImplemented. No internal selection,
//! rewrite, attempt preparation or outcome hook is exposed as a public prototype.

mod lifecycle;
mod operations;

use crate::{ConfigRevision, UsageReport};
use gproxy_channel::ChannelError;
use gproxy_protocol::{
    HttpBody, Operation, WireResponse,
    capability::{CapabilityFuture, UpstreamConnection},
};

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("core function `{0}` is not implemented")]
    NotImplemented(&'static str),
    #[error("invalid execution target: {0}")]
    InvalidTarget(String),
    #[error("operation mismatch: expected {expected:?}, got {actual:?}")]
    OperationMismatch {
        expected: Operation,
        actual: Operation,
    },
    #[error("no usable credential in the supplied candidate set")]
    NoUsableCredential,
    #[error("request cancelled")]
    Cancelled,
    #[error("execution deadline exceeded")]
    DeadlineExceeded,
    #[error("rewrite failed: {0}")]
    Rewrite(String),
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error(transparent)]
    Cache(#[from] gproxy_cache::CacheError),
    #[error(transparent)]
    Store(#[from] gproxy_store::StoreError),
}

/// Must resolve when the response stream/socket finishes or is dropped, without
/// requiring a second read of its body. Native futures are Send; WASM follows
/// protocol's native future contract. The producer is not implemented yet.
pub type UsageCompletion = CapabilityFuture<'static, CoreResult<UsageReport>>;

/// Transport response and eventual observed usage are separate. Returning
/// response headers is not completion of a streaming invocation.
pub struct Execution<T> {
    pub response: T,
    pub usage: UsageCompletion,
}
pub type HttpExecution = Execution<WireResponse<HttpBody>>;
pub type WebSocketExecution = Execution<UpstreamConnection>;

/// Successful publication result. A delayed reload may legitimately load an
/// older revision and leave the already-newer active snapshot untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReloadOutcome {
    pub loaded_revision: ConfigRevision,
    pub active_revision: ConfigRevision,
    pub published: bool,
}

/// Public credential status contains no decrypted or sealed secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialStatus {
    pub credential_id: String,
    pub version: i64,
    pub expires_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RefreshMode {
    #[default]
    IfNeeded,
    Force,
}
