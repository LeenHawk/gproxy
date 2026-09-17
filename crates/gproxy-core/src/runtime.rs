//! Mutable credential material and serializable shared-cache payloads. These
//! types do not acquire locks, choose targets, refresh tokens or publish events.

use crate::{ConfigRevision, SessionSource};
use arc_swap::ArcSwap;
use gproxy_protocol::{Dialect, Operation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// One coherent decrypted version. Never Debug/Serialize and never a cache
/// notification payload. Persist the whole replacement before publishing it.
pub struct CredentialVersion {
    pub version: i64,
    pub expires_at_ms: Option<i64>,
    pub secret: serde_json::Value,
}
pub struct CredentialState {
    current: ArcSwap<CredentialVersion>,
}
impl CredentialState {
    pub fn new(current: Arc<CredentialVersion>) -> Self {
        Self {
            current: ArcSwap::from(current),
        }
    }
    /// Each attempt pins this Arc; a concurrent refresh cannot change it in place.
    pub fn load(&self) -> Arc<CredentialVersion> {
        self.current.load_full()
    }
    /// Only use after successful Store CAS or an authoritative Store read.
    /// No refresh lease is acquired and no persistence is performed here.
    pub fn publish_if_newer(&self, next: Arc<CredentialVersion>) -> bool {
        let previous = self.current.rcu(|current| {
            if next.version > current.version {
                next.clone()
            } else {
                current.clone()
            }
        });
        next.version > previous.version
    }
}

/// A single canonical subject scope, shared by header and body forms of a
/// client's identity. Key serialization/namespacing belongs to future runtime.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AffinityScope {
    /// Opaque, authenticated caller isolation scope supplied by the upper layer.
    pub scope: String,
    pub session_id: String,
    pub source: SessionSource,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CredentialAffinityKey {
    pub scope: AffinityScope,
    pub provider_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CredentialAffinity {
    pub credential_id: String,
    pub anchored_at_ms: i64,
    pub last_success_at_ms: i64,
}
/// Credential rotation progress for the permitted candidate set only.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CredentialSelectionState {
    pub candidate_signature: String,
    pub next: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HealthKey {
    pub provider_id: String,
    pub credential_id: String,
    /// None denotes a credential-wide observation across models.
    pub upstream_model: Option<String>,
    pub operation: Option<Operation>,
    pub dialect: Option<Dialect>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthState {
    pub consecutive_failures: u32,
    pub cooldown_until_ms: Option<i64>,
    pub last_success_at_ms: Option<i64>,
    pub last_failure_at_ms: Option<i64>,
}

/// Application notification payloads; cache remains generic bytes. Consumers
/// must reconcile with durable state on subscribe, lag, reconnect and polling.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Invalidation {
    ConfigurationChanged {
        revision: ConfigRevision,
    },
    CredentialChanged {
        credential_id: String,
        version: i64,
    },
    /// Authoritative loading decides removal/replacement; no secret in the event.
    CredentialRemoved {
        credential_id: String,
        revision: ConfigRevision,
    },
}
