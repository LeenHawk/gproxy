//! Upstream answer classification. Moved down from the core: deciding what
//! a provider's response *means* is channel knowledge.

/// How one upstream attempt ended. Drives the engine's failover loop and
/// credential health accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Usable answer; relay it.
    Success,
    /// Transient upstream failure (429, 5xx, overload): worth the next
    /// credential if the budget allows.
    Retryable,
    /// The client must see this (4xx semantics, content policy): relaying
    /// beats retrying.
    Terminal,
    /// The credential itself is dead (revoked, expired beyond refresh):
    /// mark it and never retry it in this request.
    CredentialDead,
}

impl Disposition {
    pub fn should_failover(self) -> bool {
        matches!(self, Self::Retryable | Self::CredentialDead)
    }
}
