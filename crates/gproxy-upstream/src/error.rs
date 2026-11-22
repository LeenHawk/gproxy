use gproxy_protocol::{
    OperationKey,
    capability::{CapabilityError, CapabilityFuture},
};

#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("not supported by this upstream")]
    UnsupportedService,
    #[error("invalid upstream response: {0}")]
    InvalidResponse(String),
    #[error("upstream returned {status}")]
    UpstreamResponse {
        status: http::StatusCode,
        body: gproxy_protocol::connection::Bytes,
    },
    #[error("operation is not supported: {0:?}")]
    UnsupportedOperation(OperationKey),
    #[error("wrong transport for operation: {0:?}")]
    WrongTransport(OperationKey),
    #[error("invalid channel configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid credential")]
    InvalidCredential,
    /// The upstream definitively refused to refresh this credential
    /// (invalid_grant, revoked or expired refresh token). Retrying later does
    /// not help; the host remembers the failure according to the upstream
    /// kind's rejection policy until the credential changes.
    /// Transient transport or 5xx failures must not use this variant.
    #[error("credential refresh rejected: {0}")]
    RefreshRejected(String),
    /// A continuation of an earlier request is held by another host process
    /// (a live upstream connection cannot move). The host routes the request
    /// to that instance; nothing about the credential is wrong.
    #[error("continuation is held by instance `{instance_id}`")]
    ContinuationElsewhere { instance_id: String },
    /// The host's caller facts (`ServiceCaller`) could not be read or written.
    #[error("host service failed: {0}")]
    Host(String),
    #[error(transparent)]
    Transport(#[from] CapabilityError),
}

pub type UpstreamFuture<'a, T> = CapabilityFuture<'a, Result<T, UpstreamError>>;
