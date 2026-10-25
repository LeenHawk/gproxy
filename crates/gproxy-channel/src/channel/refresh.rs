//! Optional credential refresh. Nothing here performs automatic refresh or writes
//! to storage: the host serializes refreshes and persists rotations atomically.

use gproxy_protocol::capability::CapabilityFuture;
use serde_json::Value;

use super::{ChannelError, ConnectionPurpose, CredentialContext, CredentialView};

pub type RefreshContext<'a> = CredentialContext<'a>;

/// Caller-reported reason for contacting the token authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshReason {
    ProactiveExpiry,
    Scheduled,
    UnauthorizedRecovery,
    Forced,
}

/// A full replacement, never a partial secret merge. Intentionally no Debug or
/// Serialize. Persist secret + expiry + optional metadata with a CAS on the input credential version;
/// rotating refresh tokens must be durable before the new credential is used.
pub struct CredentialUpdate {
    pub secret: Value,
    /// Full public metadata replacement; None preserves existing metadata.
    pub metadata: Option<Value>,
    pub expires_at_ms: Option<i64>,
}

pub trait CredentialRefresh: Send + Sync {
    /// Default request-time renewal: access material within five minutes, or
    /// refresh material within one day. Channels may supply their own schedule.
    fn proactive_refresh_reason(
        &self,
        credential: CredentialView<'_>,
        now_ms: i64,
    ) -> Option<RefreshReason> {
        (credential
            .expires_at_ms
            .is_some_and(|expires| expires <= now_ms.saturating_add(300_000))
            || credential
                .secret
                .get("refresh_expires_at_ms")
                .and_then(Value::as_i64)
                .is_some_and(|expires| expires <= now_ms.saturating_add(86_400_000)))
        .then_some(RefreshReason::ProactiveExpiry)
    }

    /// Most channels persist definitive rejection as Dead. A channel matching
    /// an in-memory auth manager may instead remember a rejection `reason` for
    /// this version only, leaving durable credentials untouched and reloading
    /// before another try.
    fn persist_rejection(&self, _reason: &str) -> bool {
        true
    }

    /// Consume an upstream credential-rotation header. The host seals and
    /// version-CAS persists the replacement before publishing it. This hook
    /// must remove secret response headers even when no update is accepted.
    fn response_update(
        &self,
        _credential: CredentialView<'_>,
        _request: &http::Request<()>,
        _headers: &mut http::HeaderMap,
    ) -> Option<CredentialUpdate> {
        None
    }

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

    fn refresh_with_reason<'a>(
        &'a self,
        context: RefreshContext<'a>,
        _reason: RefreshReason,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>> {
        self.refresh(context)
    }
}
