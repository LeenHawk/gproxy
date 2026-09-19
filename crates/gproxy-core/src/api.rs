//! Public operation and execution-data lifecycle API. Operation wrappers validate
//! their identity and share HTTP/WS dispatch; actual execution, Store assembly
//! and account capabilities still return NotImplemented. No internal selection,
//! rewrite, attempt preparation or outcome hook is exposed as a public prototype.

pub(crate) mod lifecycle;
mod operations;

use crate::{ConfigRevision, CredentialStatus, UsageReport};
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
    /// The caller's role does not permit what it asked for (an admin-only
    /// service view). The host authenticates; core only checks the pairing.
    #[error("forbidden: {0}")]
    Forbidden(&'static str),
    /// Another instance held the refresh lease for the whole wait and no
    /// newer durable version appeared.
    #[error("credential `{credential_id}` is being refreshed elsewhere")]
    RefreshContended { credential_id: String },
    /// The request continues an earlier one whose live upstream connection is
    /// held by another host process. Route it there; the credential is fine
    /// and nothing was retried.
    #[error("continuation is held by instance `{instance_id}`")]
    ContinuationElsewhere { instance_id: String },
    /// The credential is persisted as Dead. Waiting or retrying does not help;
    /// a person must log in again. `reason` is the Store's status_reason.
    #[error("credential `{credential_id}` is dead: {}", reason.as_deref().unwrap_or("unknown"))]
    CredentialDead {
        credential_id: String,
        reason: Option<String>,
    },
    #[error("request cancelled")]
    Cancelled,
    #[error("execution deadline exceeded")]
    DeadlineExceeded,
    #[error("rewrite failed: {0}")]
    Rewrite(String),
    #[error(transparent)]
    Route(#[from] crate::convert::RouteError),
    /// Protocol conversion or adaptation failed. `kind()` distinguishes a bad
    /// client request from an unsupported pair or a host/transport fault.
    #[error(transparent)]
    Transform(#[from] gproxy_protocol::transform::TransformError),
    #[error(transparent)]
    Secret(#[from] crate::SecretError),
    #[error(transparent)]
    Limits(#[from] crate::LimitsError),
    #[error(transparent)]
    Assembly(#[from] crate::AssemblyError),
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

/// Proof that a response has been armed for settlement. Only the execution
/// funnel constructs it, so an `Execution` cannot exist without a funnel.
pub struct Settled(pub(crate) ());

/// Transport response and eventual observed usage are separate. Returning
/// response headers is not completion of a streaming invocation. Only the
/// funnel can construct one: every path that yields an Execution has armed
/// settlement for the response it carries.
pub struct Execution<T> {
    response: T,
    usage: UsageCompletion,
}
impl<T: std::fmt::Debug> std::fmt::Debug for Execution<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Execution")
            .field("response", &self.response)
            .field("usage", &"UsageCompletion { .. }")
            .finish()
    }
}
impl<T> Execution<T> {
    pub(crate) fn new(response: T, usage: UsageCompletion, _settled: Settled) -> Self {
        Self { response, usage }
    }
    pub fn response(&self) -> &T {
        &self.response
    }
    pub fn into_parts(self) -> (T, UsageCompletion) {
        (self.response, self.usage)
    }
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

/// Public credential summary; contains no decrypted or sealed secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialSummary {
    pub credential_id: String,
    pub version: i64,
    pub expires_at_ms: Option<i64>,
    pub status: CredentialStatus,
    pub status_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RefreshMode {
    #[default]
    IfNeeded,
    Force,
}
