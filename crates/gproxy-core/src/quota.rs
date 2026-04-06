//! Quota service: user cost quota management with pre-hold/settle support.

use dashmap::DashMap;

/// Manages user cost quotas with pre-hold/settle semantics.
///
/// Maintains both the quota configuration (total allocated) and runtime
/// cost tracking (used amount). Supports the pre-hold pattern where cost
/// is estimated and reserved before request execution, then settled with
/// actual cost after response.
pub struct QuotaService {
    /// (quota_total, cost_used) per user.
    quotas: DashMap<i64, (f64, f64)>,
}

impl QuotaService {
    /// Creates a new empty quota service.
    pub fn new() -> Self {
        Self {
            quotas: DashMap::new(),
        }
    }

    /// Get user quota info: (quota_total, cost_used). Returns (0, 0) if not set.
    pub fn get_quota(&self, user_id: i64) -> (f64, f64) {
        self.quotas
            .get(&user_id)
            .map(|e| *e.value())
            .unwrap_or((0.0, 0.0))
    }

    /// Check if user has remaining quota. Returns error if exhausted.
    pub fn check_quota(&self, user_id: i64) -> Result<(), QuotaExhausted> {
        let (quota, cost_used) = self.get_quota(user_id);
        if quota > 0.0 && cost_used >= quota {
            return Err(QuotaExhausted { quota, cost_used });
        }
        Ok(())
    }

    /// Atomically add cost to a user's usage. Returns (quota_total, new_cost_used).
    pub fn add_cost(&self, user_id: i64, cost: f64) -> (f64, f64) {
        let mut entry = self.quotas.entry(user_id).or_insert((0.0, 0.0));
        entry.1 += cost;
        *entry.value()
    }

    /// Set or update a user's quota and cost_used.
    pub fn upsert(&self, user_id: i64, quota: f64, cost_used: f64) {
        self.quotas.insert(user_id, (quota, cost_used));
    }

    /// Replace all quotas atomically (bootstrap/reload).
    pub fn replace_all(&self, quotas: Vec<(i64, f64, f64)>) {
        self.quotas.clear();
        for (user_id, quota, cost_used) in quotas {
            self.quotas.insert(user_id, (quota, cost_used));
        }
    }

    /// Get a snapshot of all quotas for reconciliation.
    pub fn snapshot(&self) -> Vec<(i64, f64, f64)> {
        self.quotas
            .iter()
            .map(|e| (*e.key(), e.value().0, e.value().1))
            .collect()
    }
}

impl Default for QuotaService {
    fn default() -> Self {
        Self::new()
    }
}

/// Error returned when a user's quota is exhausted.
#[derive(Debug, Clone)]
pub struct QuotaExhausted {
    /// Total quota allocated.
    pub quota: f64,
    /// Amount already consumed.
    pub cost_used: f64,
}

impl std::fmt::Display for QuotaExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "quota exhausted: allocated={}, used={}",
            self.quota, self.cost_used
        )
    }
}
