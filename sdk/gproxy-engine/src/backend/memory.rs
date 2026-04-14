use std::future::ready;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;

use crate::backend::traits::{AffinityBackend, QuotaBackend, QuotaHold, RateLimitBackend};
use crate::backend::types::{
    BackendError, QuotaBalance, QuotaError, QuotaExhausted, RateLimitExceeded, RateLimitWindow,
};

/// In-memory rate-limit backend for tests and single-process use.
#[derive(Debug, Default)]
pub struct InMemoryRateLimit {
    counters: DashMap<(String, u64), AtomicU64>,
}

impl InMemoryRateLimit {
    /// Creates a new empty in-memory rate-limit backend.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes all counters whose epoch has passed, freeing memory.
    ///
    /// Call this periodically (e.g. every 60s) from a background worker
    /// to prevent unbounded growth of the internal counters map.
    pub fn purge_expired(&self) {
        let now = SystemTime::now();
        let minute_epoch = rate_limit_epoch(RateLimitWindow::PerMinute { limit: 0 }, now);
        let day_epoch = rate_limit_epoch(RateLimitWindow::PerDay { limit: 0 }, now);
        self.counters.retain(|(_key, epoch), _counter| {
            // Keep entries from the current epoch; remove older ones.
            // We don't know which window the entry belongs to, so keep if
            // it matches either the current minute or current day epoch.
            *epoch >= minute_epoch || *epoch >= day_epoch
        });
    }
}

impl RateLimitBackend for InMemoryRateLimit {
    fn try_acquire(
        &self,
        key: &str,
        window: RateLimitWindow,
    ) -> impl std::future::Future<Output = Result<u64, RateLimitExceeded>> + Send {
        let now = SystemTime::now();
        let epoch = rate_limit_epoch(window, now);
        let retry_after = rate_limit_retry_after(window, now);
        let limit = rate_limit_limit(window);
        let counter = self
            .counters
            .entry((key.to_string(), epoch))
            .or_insert_with(|| AtomicU64::new(0));

        loop {
            let current = counter.load(Ordering::Relaxed);
            if current >= limit {
                return ready(Err(RateLimitExceeded {
                    retry_after,
                    window,
                }));
            }

            let next = current + 1;
            if counter
                .compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return ready(Ok(next));
            }
        }
    }

    fn current_count(
        &self,
        key: &str,
        window: RateLimitWindow,
    ) -> impl std::future::Future<Output = u64> + Send {
        let epoch = rate_limit_epoch(window, SystemTime::now());
        let count = self
            .counters
            .get(&(key.to_string(), epoch))
            .map(|entry| entry.load(Ordering::Relaxed))
            .unwrap_or(0);
        ready(count)
    }
}

/// In-memory quota backend for tests and single-process use.
#[derive(Debug, Clone, Default)]
pub struct InMemoryQuota {
    states: Arc<DashMap<i64, QuotaState>>,
}

impl InMemoryQuota {
    /// Creates a new empty in-memory quota backend.
    pub fn new() -> Self {
        Self::default()
    }
}

impl QuotaBackend for InMemoryQuota {
    type Hold = InMemoryQuotaHold;

    fn try_reserve(
        &self,
        identity_id: i64,
        estimated_cost: u64,
    ) -> impl std::future::Future<Output = Result<Self::Hold, QuotaExhausted>> + Send {
        let mut state = self.states.entry(identity_id).or_default();
        let remaining = state.remaining();
        if remaining < estimated_cost {
            return ready(Err(QuotaExhausted {
                remaining,
                requested: estimated_cost,
            }));
        }

        state.reserved = state.reserved.saturating_add(estimated_cost);
        drop(state);

        ready(Ok(InMemoryQuotaHold {
            states: Arc::clone(&self.states),
            identity_id,
            reserved_amount: estimated_cost,
            settled: AtomicBool::new(false),
        }))
    }

    fn balance(
        &self,
        identity_id: i64,
    ) -> impl std::future::Future<Output = Result<QuotaBalance, QuotaError>> + Send {
        let state = self
            .states
            .get(&identity_id)
            .map(|entry| *entry.value())
            .unwrap_or_default();
        ready(Ok(QuotaBalance::from(state)))
    }

    fn set_quota(
        &self,
        identity_id: i64,
        total: u64,
    ) -> impl std::future::Future<Output = Result<(), QuotaError>> + Send {
        let mut state = self.states.entry(identity_id).or_default();
        state.total = total;
        ready(Ok(()))
    }
}

/// Reservation handle returned by [`InMemoryQuota`].
#[derive(Debug)]
pub struct InMemoryQuotaHold {
    states: Arc<DashMap<i64, QuotaState>>,
    identity_id: i64,
    reserved_amount: u64,
    settled: AtomicBool,
}

impl QuotaHold for InMemoryQuotaHold {
    fn settle(
        self,
        actual_cost: u64,
    ) -> impl std::future::Future<Output = Result<(), QuotaError>> + Send {
        if !self.settled.swap(true, Ordering::AcqRel) {
            self.apply_finalization(actual_cost);
        }
        ready(Ok(()))
    }
}

/// In-memory affinity backend for tests and single-process use.
#[derive(Debug, Default)]
pub struct InMemoryAffinity {
    bindings: DashMap<String, (String, Instant)>,
}

impl InMemoryAffinity {
    /// Creates a new empty in-memory affinity backend.
    pub fn new() -> Self {
        Self::default()
    }
}

impl AffinityBackend for InMemoryAffinity {
    fn get_binding(&self, key: &str) -> impl std::future::Future<Output = Option<String>> + Send {
        let binding = if let Some(entry) = self.bindings.get(key) {
            if entry.value().1 <= Instant::now() {
                drop(entry);
                self.bindings.remove(key);
                None
            } else {
                Some(entry.value().0.clone())
            }
        } else {
            None
        };
        ready(binding)
    }

    fn set_binding(
        &self,
        key: &str,
        credential_id: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = Result<(), BackendError>> + Send {
        self.bindings.insert(
            key.to_string(),
            (credential_id.to_string(), Instant::now() + ttl),
        );
        ready(Ok(()))
    }

    fn remove_binding(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = Result<(), BackendError>> + Send {
        self.bindings.remove(key);
        ready(Ok(()))
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct QuotaState {
    total: u64,
    used: u64,
    reserved: u64,
}

impl QuotaState {
    fn remaining(self) -> u64 {
        self.total
            .saturating_sub(self.used.saturating_add(self.reserved))
    }
}

impl From<QuotaState> for QuotaBalance {
    fn from(value: QuotaState) -> Self {
        Self {
            total: value.total,
            used: value.used,
            reserved: value.reserved,
        }
    }
}

impl InMemoryQuotaHold {
    fn apply_finalization(&self, actual_cost: u64) {
        match self.states.entry(self.identity_id) {
            Entry::Occupied(mut entry) => {
                let state = entry.get_mut();
                state.reserved = state.reserved.saturating_sub(self.reserved_amount);
                state.used = state.used.saturating_add(actual_cost);
            }
            Entry::Vacant(entry) => {
                entry.insert(QuotaState {
                    total: 0,
                    used: actual_cost,
                    reserved: 0,
                });
            }
        }
    }
}

impl Drop for InMemoryQuotaHold {
    fn drop(&mut self) {
        if !self.settled.swap(true, Ordering::AcqRel) {
            self.apply_finalization(self.reserved_amount);
        }
    }
}

fn rate_limit_limit(window: RateLimitWindow) -> u64 {
    match window {
        RateLimitWindow::PerMinute { limit } | RateLimitWindow::PerDay { limit } => limit,
    }
}

fn rate_limit_epoch(window: RateLimitWindow, now: SystemTime) -> u64 {
    elapsed_since_epoch(now).as_secs() / rate_limit_window_seconds(window)
}

fn rate_limit_retry_after(window: RateLimitWindow, now: SystemTime) -> Duration {
    let window_seconds = rate_limit_window_seconds(window);
    let elapsed = elapsed_since_epoch(now).as_secs();
    let elapsed_in_window = elapsed % window_seconds;
    Duration::from_secs(window_seconds.saturating_sub(elapsed_in_window).max(1))
}

fn rate_limit_window_seconds(window: RateLimitWindow) -> u64 {
    match window {
        RateLimitWindow::PerMinute { .. } => 60,
        RateLimitWindow::PerDay { .. } => 86_400,
    }
}

fn elapsed_since_epoch(now: SystemTime) -> Duration {
    match now.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration,
        Err(_) => Duration::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::backend::traits::{AffinityBackend, QuotaBackend, QuotaHold, RateLimitBackend};
    use crate::backend::types::RateLimitWindow;

    use super::{InMemoryAffinity, InMemoryQuota, InMemoryRateLimit};

    #[tokio::test]
    async fn in_memory_rate_limit_enforces_limits() {
        let backend = InMemoryRateLimit::new();
        let window = RateLimitWindow::PerMinute { limit: 2 };

        let first = match backend.try_acquire("alpha", window).await {
            Ok(count) => count,
            Err(err) => panic!("expected first acquire to succeed, got {err}"),
        };
        assert_eq!(first, 1);

        let second = match backend.try_acquire("alpha", window).await {
            Ok(count) => count,
            Err(err) => panic!("expected second acquire to succeed, got {err}"),
        };
        assert_eq!(second, 2);

        let error = match backend.try_acquire("alpha", window).await {
            Ok(count) => panic!("expected limit exhaustion, got count {count}"),
            Err(err) => err,
        };
        assert_eq!(error.window, window);
        assert!(error.retry_after > Duration::ZERO);

        let count = backend.current_count("alpha", window).await;
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn in_memory_quota_tracks_reservations_and_settlement() {
        let backend = InMemoryQuota::new();

        if let Err(err) = backend.set_quota(7, 10).await {
            panic!("expected quota set to succeed, got {err}");
        }

        let hold = match backend.try_reserve(7, 4).await {
            Ok(hold) => hold,
            Err(err) => panic!("expected reservation to succeed, got {err}"),
        };

        let reserved = match backend.balance(7).await {
            Ok(balance) => balance,
            Err(err) => panic!("expected balance query to succeed, got {err}"),
        };
        assert_eq!(reserved.total, 10);
        assert_eq!(reserved.used, 0);
        assert_eq!(reserved.reserved, 4);

        if let Err(err) = hold.settle(3).await {
            panic!("expected settlement to succeed, got {err}");
        }

        let settled = match backend.balance(7).await {
            Ok(balance) => balance,
            Err(err) => panic!("expected balance query to succeed, got {err}"),
        };
        assert_eq!(settled.total, 10);
        assert_eq!(settled.used, 3);
        assert_eq!(settled.reserved, 0);
    }

    #[tokio::test]
    async fn in_memory_quota_hold_drop_conservatively_debits_reserved_amount() {
        let backend = InMemoryQuota::new();

        if let Err(err) = backend.set_quota(11, 8).await {
            panic!("expected quota set to succeed, got {err}");
        }

        let hold = match backend.try_reserve(11, 5).await {
            Ok(hold) => hold,
            Err(err) => panic!("expected reservation to succeed, got {err}"),
        };

        drop(hold);

        let balance = match backend.balance(11).await {
            Ok(balance) => balance,
            Err(err) => panic!("expected balance query to succeed, got {err}"),
        };
        assert_eq!(balance.total, 8);
        assert_eq!(balance.used, 5);
        assert_eq!(balance.reserved, 0);
    }

    #[tokio::test]
    async fn in_memory_affinity_respects_expiration_and_removal() {
        let backend = InMemoryAffinity::new();

        if let Err(err) = backend
            .set_binding("prompt", "cred-a", Duration::from_millis(15))
            .await
        {
            panic!("expected set binding to succeed, got {err}");
        }

        let binding = backend.get_binding("prompt").await;
        assert_eq!(binding.as_deref(), Some("cred-a"));

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(backend.get_binding("prompt").await, None);

        if let Err(err) = backend
            .set_binding("prompt", "cred-b", Duration::from_secs(1))
            .await
        {
            panic!("expected set binding to succeed, got {err}");
        }

        if let Err(err) = backend.remove_binding("prompt").await {
            panic!("expected remove binding to succeed, got {err}");
        }
        assert_eq!(backend.get_binding("prompt").await, None);
    }
}
