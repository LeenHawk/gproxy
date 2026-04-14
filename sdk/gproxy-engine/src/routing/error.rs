use thiserror::Error;

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Errors produced by framework-independent routing helpers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoutingError {
    /// The request shape does not map to a supported route.
    #[error("{0}")]
    Unsupported(&'static str),
    /// A routing helper could not decode a JSON payload it needed to inspect.
    #[error("failed to decode {kind} json for ({operation:?}, {protocol:?}): {message}")]
    JsonDecode {
        /// The JSON payload kind that failed to decode.
        kind: &'static str,
        /// The inferred operation family associated with the payload.
        operation: OperationFamily,
        /// The inferred protocol kind associated with the payload.
        protocol: ProtocolKind,
        /// The original decode failure message.
        message: String,
    },
    /// A provider-prefixed model string was inconsistent or invalid.
    #[error("provider prefix error: {message}")]
    ProviderPrefix {
        /// A human-readable description of the provider-prefix failure.
        message: String,
    },
}
