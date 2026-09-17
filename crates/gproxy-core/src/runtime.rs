//! Mutable credential material and serializable shared-cache payloads. These
//! types do not acquire locks, choose targets, refresh tokens or publish events.

use crate::{ConfigRevision, SessionSource};
use arc_swap::ArcSwap;
use gproxy_channel::channel::QuotaScope;
use gproxy_protocol::Operation;
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

/// Where a block came from. Quota blocks end at the dimension's reset; failure
/// and rate-limit blocks end at a cooldown chosen by core.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockSource {
    /// A Reported dimension was observed exhausted via QuotaQuery, QuotaHeaders
    /// or a confirmed exhaustion reply. `dimension` is the channel's
    /// QuotaDimension id; `cycle_id` references the persisted CredentialQuotaCycle.
    QuotaExhausted {
        dimension: String,
        cycle_id: Option<String>,
    },
    /// A Counted dimension's window reached its declared limit.
    Counted { dimension: String },
    /// Upstream rate limiting with a retry-after, not a quota dimension.
    RateLimited,
    /// Consecutive failures for this scope tripped a cooldown.
    Failures { consecutive: u32 },
}

/// One reason a credential is unusable for part of its model/operation space.
/// Persisted one row per block in Store `credential_blocks` (scope and source
/// as JSON); the cache copy is the hot path and is rebuilt from Store on load.
/// The scope is the upstream's own vocabulary; core never expands a family
/// prefix into a model list. `Unknown` scope blocks the whole credential.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialBlock {
    pub scope: QuotaScope,
    /// None applies to every operation.
    pub operation: Option<Operation>,
    pub until_ms: i64,
    pub source: BlockSource,
    pub observed_at_ms: i64,
}
impl CredentialBlock {
    /// A request without a model (e.g. listing models) is only affected by
    /// credential-wide blocks.
    pub fn applies_to(&self, upstream_model: Option<&str>, operation: Operation) -> bool {
        if self.operation.is_some_and(|blocked| blocked != operation) {
            return false;
        }
        match (&self.scope, upstream_model) {
            (QuotaScope::All | QuotaScope::Unknown, _) => true,
            (scope, Some(model)) => scope.matches(model),
            (_, None) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CredentialBlocksKey {
    pub provider_id: String,
    pub credential_id: String,
}
/// Consecutive failures for one scope/operation. A streak trips a `Failures`
/// block for the same scope; success on that scope resets only that streak,
/// so a broken model does not hide behind a healthy one and vice versa.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureStreak {
    pub scope: QuotaScope,
    pub operation: Option<Operation>,
    pub consecutive: u32,
    pub last_failure_at_ms: i64,
}

/// Availability of one credential: the cache payload consulted by selection.
/// Blocks mirror Store rows; failure streaks are cache-only counters toward
/// the next block. It is not a quota ledger; observed cycles live in Store and
/// remaining capacity stays in the channel's QuotaSnapshot. Durable lifecycle
/// (Dead) is `CredentialData.status`, not a block. Expired blocks are pruned on
/// write; readers still compare `until_ms`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CredentialBlocks {
    pub blocks: Vec<CredentialBlock>,
    pub failures: Vec<FailureStreak>,
    pub last_success_at_ms: Option<i64>,
}
impl CredentialBlocks {
    /// The longest-lasting block covering this model/operation at `now_ms`.
    pub fn blocked_by(
        &self,
        upstream_model: Option<&str>,
        operation: Operation,
        now_ms: i64,
    ) -> Option<&CredentialBlock> {
        self.blocks
            .iter()
            .filter(|block| block.until_ms > now_ms && block.applies_to(upstream_model, operation))
            .max_by_key(|block| block.until_ms)
    }
    /// The streak recorded for exactly this scope/operation pair.
    pub fn streak(
        &self,
        scope: &QuotaScope,
        operation: Option<Operation>,
    ) -> Option<&FailureStreak> {
        self.failures
            .iter()
            .find(|streak| &streak.scope == scope && streak.operation == operation)
    }
}

/// Fixed-window counter for one Counted QuotaDimension, kept in the cache via
/// `Cache::increment` with the window's remaining TTL. `window_start_ms` is
/// derived from the dimension's QuotaWindow; Total uses 0.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CountedWindowKey {
    pub credential_id: String,
    pub dimension: String,
    pub window_start_ms: i64,
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
