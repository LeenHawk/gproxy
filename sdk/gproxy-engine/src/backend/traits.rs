use std::future::Future;
use std::time::Duration;

use crate::backend::types::{
    BackendError, QuotaBalance, QuotaError, QuotaExhausted, RateLimitExceeded, RateLimitWindow,
};

/// Backend for distributed or local rate-limit accounting.
pub trait RateLimitBackend: Send + Sync + 'static {
    /// Attempts to consume one request from the given window and returns the new count.
    fn try_acquire(
        &self,
        key: &str,
        window: RateLimitWindow,
    ) -> impl Future<Output = Result<u64, RateLimitExceeded>> + Send;

    /// Returns the current observed request count for the active window.
    fn current_count(&self, key: &str, window: RateLimitWindow)
    -> impl Future<Output = u64> + Send;
}

/// Backend for reserving and settling spend quota.
pub trait QuotaBackend: Send + Sync + 'static {
    /// Reservation token returned by a successful quota hold.
    type Hold: QuotaHold;

    /// Attempts to reserve the estimated cost for an identity.
    fn try_reserve(
        &self,
        identity_id: i64,
        estimated_cost: u64,
    ) -> impl Future<Output = Result<Self::Hold, QuotaExhausted>> + Send;

    /// Returns the current quota totals for an identity.
    fn balance(
        &self,
        identity_id: i64,
    ) -> impl Future<Output = Result<QuotaBalance, QuotaError>> + Send;

    /// Sets the total quota for an identity.
    fn set_quota(
        &self,
        identity_id: i64,
        total: u64,
    ) -> impl Future<Output = Result<(), QuotaError>> + Send;
}

/// Settlement handle returned by a quota reservation.
pub trait QuotaHold: Send + 'static {
    /// Finalizes a reservation with the actual observed cost.
    fn settle(self, actual_cost: u64) -> impl Future<Output = Result<(), QuotaError>> + Send;
}

/// Backend for credential affinity bindings with expiration.
pub trait AffinityBackend: Send + Sync + 'static {
    /// Returns the bound credential id for a key if the binding is still valid.
    fn get_binding(&self, key: &str) -> impl Future<Output = Option<String>> + Send;

    /// Stores or refreshes a binding for the provided key until the TTL expires.
    fn set_binding(
        &self,
        key: &str,
        credential_id: &str,
        ttl: Duration,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Removes any existing binding for the provided key.
    fn remove_binding(&self, key: &str) -> impl Future<Output = Result<(), BackendError>> + Send;
}
