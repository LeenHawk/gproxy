//! Runtime-selectable backend implementations using enum dispatch.
//!
//! Each enum wraps InMemory (default) and optionally Redis variants,
//! delegating trait calls to the active variant. This avoids trait objects
//! while allowing runtime backend selection at bootstrap.

use std::future::Future;
use std::time::Duration;

use gproxy_sdk::provider::backend::memory::{
    InMemoryAffinity, InMemoryQuota, InMemoryQuotaHold, InMemoryRateLimit,
};
use gproxy_sdk::provider::backend::traits::{
    AffinityBackend, QuotaBackend, QuotaHold, RateLimitBackend,
};
use gproxy_sdk::provider::backend::types::{
    BackendError, QuotaBalance, QuotaError, QuotaExhausted, RateLimitExceeded, RateLimitWindow,
};

// ---------------------------------------------------------------------------
// RateLimit dispatch
// ---------------------------------------------------------------------------

/// Runtime-selectable rate limit backend.
pub enum RateLimitDispatch {
    /// In-memory (single instance, default).
    Memory(InMemoryRateLimit),
    /// Redis (multi-instance).
    #[cfg(feature = "redis")]
    Redis(crate::redis_backend::RedisRateLimit),
}

#[allow(clippy::manual_async_fn)]
impl RateLimitBackend for RateLimitDispatch {
    fn try_acquire(
        &self,
        key: &str,
        window: RateLimitWindow,
    ) -> impl Future<Output = Result<u64, RateLimitExceeded>> + Send {
        async move {
            match self {
                Self::Memory(m) => RateLimitBackend::try_acquire(m, key, window).await,
                #[cfg(feature = "redis")]
                Self::Redis(r) => RateLimitBackend::try_acquire(r, key, window).await,
            }
        }
    }

    fn current_count(
        &self,
        key: &str,
        window: RateLimitWindow,
    ) -> impl Future<Output = u64> + Send {
        async move {
            match self {
                Self::Memory(m) => RateLimitBackend::current_count(m, key, window).await,
                #[cfg(feature = "redis")]
                Self::Redis(r) => RateLimitBackend::current_count(r, key, window).await,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Quota dispatch
// ---------------------------------------------------------------------------

/// Runtime-selectable quota backend.
pub enum QuotaDispatch {
    Memory(InMemoryQuota),
    #[cfg(feature = "redis")]
    Redis(crate::redis_backend::RedisQuota),
}

/// Runtime-selectable quota hold.
pub enum QuotaHoldDispatch {
    Memory(InMemoryQuotaHold),
    #[cfg(feature = "redis")]
    Redis(crate::redis_backend::RedisQuotaHold),
}

#[allow(clippy::manual_async_fn)]
impl QuotaBackend for QuotaDispatch {
    type Hold = QuotaHoldDispatch;

    fn try_reserve(
        &self,
        identity_id: i64,
        estimated_cost: u64,
    ) -> impl Future<Output = Result<Self::Hold, QuotaExhausted>> + Send {
        async move {
            match self {
                Self::Memory(m) => QuotaBackend::try_reserve(m, identity_id, estimated_cost)
                    .await
                    .map(QuotaHoldDispatch::Memory),
                #[cfg(feature = "redis")]
                Self::Redis(r) => QuotaBackend::try_reserve(r, identity_id, estimated_cost)
                    .await
                    .map(QuotaHoldDispatch::Redis),
            }
        }
    }

    fn balance(
        &self,
        identity_id: i64,
    ) -> impl Future<Output = Result<QuotaBalance, QuotaError>> + Send {
        async move {
            match self {
                Self::Memory(m) => QuotaBackend::balance(m, identity_id).await,
                #[cfg(feature = "redis")]
                Self::Redis(r) => QuotaBackend::balance(r, identity_id).await,
            }
        }
    }

    fn set_quota(
        &self,
        identity_id: i64,
        total: u64,
    ) -> impl Future<Output = Result<(), QuotaError>> + Send {
        async move {
            match self {
                Self::Memory(m) => QuotaBackend::set_quota(m, identity_id, total).await,
                #[cfg(feature = "redis")]
                Self::Redis(r) => QuotaBackend::set_quota(r, identity_id, total).await,
            }
        }
    }
}

#[allow(clippy::manual_async_fn)]
impl QuotaHold for QuotaHoldDispatch {
    fn settle(self, actual_cost: u64) -> impl Future<Output = Result<(), QuotaError>> + Send {
        async move {
            match self {
                Self::Memory(h) => QuotaHold::settle(h, actual_cost).await,
                #[cfg(feature = "redis")]
                Self::Redis(h) => QuotaHold::settle(h, actual_cost).await,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Affinity dispatch
// ---------------------------------------------------------------------------

/// Runtime-selectable affinity backend.
pub enum AffinityDispatch {
    Memory(InMemoryAffinity),
    #[cfg(feature = "redis")]
    Redis(crate::redis_backend::RedisAffinity),
}

#[allow(clippy::manual_async_fn)]
impl AffinityBackend for AffinityDispatch {
    fn get_binding(&self, key: &str) -> impl Future<Output = Option<String>> + Send {
        async move {
            match self {
                Self::Memory(m) => AffinityBackend::get_binding(m, key).await,
                #[cfg(feature = "redis")]
                Self::Redis(r) => AffinityBackend::get_binding(r, key).await,
            }
        }
    }

    fn set_binding(
        &self,
        key: &str,
        credential_id: &str,
        ttl: Duration,
    ) -> impl Future<Output = Result<(), BackendError>> + Send {
        let cred = credential_id.to_string();
        async move {
            match self {
                Self::Memory(m) => AffinityBackend::set_binding(m, key, &cred, ttl).await,
                #[cfg(feature = "redis")]
                Self::Redis(r) => AffinityBackend::set_binding(r, key, &cred, ttl).await,
            }
        }
    }

    fn remove_binding(&self, key: &str) -> impl Future<Output = Result<(), BackendError>> + Send {
        async move {
            match self {
                Self::Memory(m) => AffinityBackend::remove_binding(m, key).await,
                #[cfg(feature = "redis")]
                Self::Redis(r) => AffinityBackend::remove_binding(r, key).await,
            }
        }
    }
}
