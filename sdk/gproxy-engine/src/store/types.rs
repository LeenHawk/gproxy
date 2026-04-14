//! Snapshot / event value types emitted by [`super::ProviderStore`].
//!
//! These are plain data shapes: read-only snapshots of provider /
//! credential state, credential update payloads, OAuth finish results,
//! and the event enum broadcast on the store's event channel. They are
//! split out of `store/mod.rs` so the main store implementation only
//! has to own the orchestration logic.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSnapshot {
    pub name: String,
    pub channel: String,
    pub settings: Value,
    pub credential_count: usize,
    pub credential_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialSnapshot {
    pub provider: String,
    pub index: usize,
    pub revision: u64,
    pub credential: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialUpdate {
    pub provider: String,
    pub index: usize,
    pub revision: u64,
    pub credential: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct OAuthFinishResult {
    pub credential: CredentialSnapshot,
    pub details: Value,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    ProviderAdded {
        name: String,
    },
    ProviderRemoved {
        name: String,
    },
    ProviderUpdated {
        name: String,
    },
    CredentialHealthChanged {
        provider: String,
        index: usize,
        status: String,
    },
}

/// Health status snapshot for a single credential.
#[derive(Debug, Clone, Serialize)]
pub struct CredentialHealthSnapshot {
    pub provider: String,
    pub index: usize,
    /// `"healthy"` or `"unavailable"`.
    pub status: String,
    /// true if `is_available(None)` returns true.
    pub available: bool,
}
