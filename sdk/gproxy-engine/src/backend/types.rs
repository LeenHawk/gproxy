use std::error::Error as StdError;
use std::time::Duration;

/// A rate-limit window with its configured request limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitWindow {
    /// Limit requests within the current UTC minute.
    PerMinute {
        /// Maximum requests allowed in one minute window.
        limit: u64,
    },
    /// Limit requests within the current UTC day.
    PerDay {
        /// Maximum requests allowed in one day window.
        limit: u64,
    },
}

/// Error returned when a rate-limit window is already exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("rate limit exceeded; retry after {retry_after:?} in {window:?}")]
pub struct RateLimitExceeded {
    /// Duration until the current window resets.
    pub retry_after: Duration,
    /// Window that rejected the acquisition.
    pub window: RateLimitWindow,
}

/// Current quota totals for a single identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuotaBalance {
    /// Total quota assigned to the identity.
    pub total: u64,
    /// Quota that has been permanently consumed.
    pub used: u64,
    /// Quota that is temporarily reserved by active holds.
    pub reserved: u64,
}

impl QuotaBalance {
    /// Returns the available quota after subtracting used and reserved amounts.
    pub fn remaining(&self) -> u64 {
        self.total
            .saturating_sub(self.used.saturating_add(self.reserved))
    }
}

/// Shared backend error wrapper for implementation-specific failures.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct BackendError {
    #[from]
    source: Box<dyn StdError + Send + Sync>,
}

/// Error returned when an estimated reservation would exceed the remaining quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("quota exhausted; remaining={remaining}, requested={requested}")]
pub struct QuotaExhausted {
    /// Remaining quota available for new reservations.
    pub remaining: u64,
    /// Requested reservation amount.
    pub requested: u64,
}

/// Error returned by quota backends.
#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    /// Reservation or settlement would exceed the remaining quota.
    #[error("quota exhausted; remaining={remaining}, requested={requested}")]
    Exhausted {
        /// Remaining quota available for new reservations.
        remaining: u64,
        /// Requested reservation amount.
        requested: u64,
    },
    /// Backend-specific storage or coordination failure.
    #[error(transparent)]
    Backend(#[from] BackendError),
}

impl From<QuotaExhausted> for QuotaError {
    fn from(value: QuotaExhausted) -> Self {
        Self::Exhausted {
            remaining: value.remaining,
            requested: value.requested,
        }
    }
}
