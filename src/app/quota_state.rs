//! Live quota accounting rows (§17).
//!
//! Quota rows carry billing-owned counters (`cost_used` and the day/week/month
//! accumulators) that settle mutates on EVERY request. The control-plane
//! snapshot only rebuilds on §7.2 invalidation, which billing never triggers —
//! so its copy of those counters freezes at the last config change and the
//! admission gate can never observe accumulated spend. These rows therefore
//! live outside the snapshot, on their own short refresh cadence.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arc_swap::ArcSwap;

use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{Quota, Scope};
use crate::util::time::unix_now_ms;

/// Refresh cadence: bounds worst-case overspend to a few seconds of traffic
/// while keeping the extra `list_all_quotas` read off all but one request per
/// interval — and off every request of a deployment with no quota rows.
const REFRESH_INTERVAL_MS: u64 = 5_000;

pub type QuotaTable = HashMap<(Scope, i64), Arc<Quota>>;

pub fn index(rows: Vec<Quota>) -> QuotaTable {
    rows.into_iter()
        .map(|q| ((q.scope, q.scope_id), Arc::new(q)))
        .collect()
}

pub struct QuotaState {
    rows: ArcSwap<QuotaTable>,
    refreshed_at_ms: AtomicU64,
}

impl QuotaState {
    /// Seed from rows known to be current (boot / snapshot rebuild).
    pub fn new(rows: QuotaTable) -> Self {
        Self {
            rows: ArcSwap::from_pointee(rows),
            refreshed_at_ms: AtomicU64::new(unix_now_ms()),
        }
    }

    /// Adopt rows that were just read from persistence.
    pub fn store(&self, rows: QuotaTable) {
        self.rows.store(Arc::new(rows));
        self.refreshed_at_ms.store(unix_now_ms(), Ordering::Relaxed);
    }

    /// Current rows, re-read from persistence when older than
    /// [`REFRESH_INTERVAL_MS`]. The refresh slot is claimed BEFORE the await so
    /// concurrent requests do not stampede the query. A failed read keeps the
    /// previous rows — the gate stays as strict as its last good view rather
    /// than opening up.
    pub async fn current(&self, db: &dyn PersistenceBackend) -> Arc<QuotaTable> {
        let now = unix_now_ms();
        let last = self.refreshed_at_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= REFRESH_INTERVAL_MS
            && self
                .refreshed_at_ms
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            match db.list_all_quotas().await {
                Ok(rows) => self.rows.store(Arc::new(index(rows))),
                Err(error) => tracing::warn!(
                    operation = "list_all_quotas",
                    error = %error,
                    "quota refresh failed; admission serves the previous rows"
                ),
            }
        }
        self.rows.load_full()
    }
}
