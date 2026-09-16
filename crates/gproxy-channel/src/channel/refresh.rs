//! Optional credential refresh. Nothing here performs automatic refresh or writes
//! to storage: the host serializes refreshes and persists rotations atomically.

use gproxy_protocol::capability::CapabilityFuture;
use serde_json::Value;

use super::{ChannelError, CredentialView, ProviderView};
use crate::client::OutboundClient;

pub struct RefreshContext<'a> {
    pub provider: ProviderView<'a>,
    pub credential: CredentialView<'a>,
    pub client: &'a dyn OutboundClient,
}

/// A full replacement, never a partial secret merge. Intentionally no Debug or
/// Serialize. Persist secret + expiry with a CAS on the input credential version;
/// rotating refresh tokens must be durable before the new credential is used.
pub struct CredentialUpdate {
    pub secret: Value,
    pub expires_at_ms: Option<i64>,
}

pub trait CredentialRefresh: Send + Sync {
    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>>;
}
