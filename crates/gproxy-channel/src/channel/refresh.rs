//! Optional credential refresh. Nothing here performs automatic refresh or writes
//! to storage: the host serializes refreshes and persists rotations atomically.

use gproxy_protocol::capability::CapabilityFuture;
use serde_json::Value;

use super::{ChannelError, ConnectionPurpose, CredentialContext, CredentialView};

pub type RefreshContext<'a> = CredentialContext<'a>;

/// A full replacement, never a partial secret merge. Intentionally no Debug or
/// Serialize. Persist secret + expiry with a CAS on the input credential version;
/// rotating refresh tokens must be durable before the new credential is used.
pub struct CredentialUpdate {
    pub secret: Value,
    pub expires_at_ms: Option<i64>,
}

pub trait CredentialRefresh: Send + Sync {
    /// Whether this credential can be renewed. Channels that also accept
    /// static API keys must distinguish those from renewable login material.
    fn supports(&self, _credential: &CredentialView<'_>) -> bool {
        true
    }

    fn connection_purpose(&self, credential: &CredentialView<'_>) -> ConnectionPurpose {
        let _ = credential;
        ConnectionPurpose::Request
    }

    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>>;
}
