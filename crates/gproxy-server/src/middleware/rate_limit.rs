use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use dashmap::DashMap;
pub use gproxy_core::RateLimitRule;

use crate::app_state::AppState;

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum RateLimitRejection {
    Rpm { limit: i32 },
    Rpd { limit: i32 },
    TotalTokens { limit: i64, requested: i64 },
    QuotaExhausted { quota: f64, cost_used: f64 },
}

// ---------------------------------------------------------------------------
// Counters (in-memory, not persisted)
// ---------------------------------------------------------------------------

const MINUTE: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(86400);

/// Sliding-window rate limit counters. Not persisted — resets on restart.
///
/// This is acceptable for single-instance deployments where the service
/// rarely restarts. RPM (60s window) recovers immediately; RPD (24h window)
/// loses at most one day of counts on restart, which is a tolerable trade-off
/// vs. the complexity of DB/Redis persistence.
///
/// If multi-instance or frequent-restart scenarios arise, consider persisting
/// RPD counters to the database or a shared store (e.g. Redis).
pub struct RateLimitCounters {
    requests: DashMap<(i64, String), RequestWindowCounter>,
}

struct RequestWindowCounter {
    minute_count: u32,
    minute_window_start: Instant,
    day_count: u32,
    day_window_start: Instant,
}

impl RateLimitCounters {
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
        }
    }

    pub fn try_acquire(
        &self,
        user_id: i64,
        model: &str,
        rpm: Option<i32>,
        rpd: Option<i32>,
    ) -> Result<(), RateLimitRejection> {
        let key = (user_id, model.to_string());
        let mut entry = self.requests.entry(key).or_insert(RequestWindowCounter {
            minute_count: 0,
            minute_window_start: Instant::now(),
            day_count: 0,
            day_window_start: Instant::now(),
        });

        if entry.minute_window_start.elapsed() >= MINUTE {
            entry.minute_count = 0;
            entry.minute_window_start = Instant::now();
        }
        if entry.day_window_start.elapsed() >= DAY {
            entry.day_count = 0;
            entry.day_window_start = Instant::now();
        }

        if let Some(limit) = rpm
            && entry.minute_count >= limit as u32
        {
            return Err(RateLimitRejection::Rpm { limit });
        }
        if let Some(limit) = rpd
            && entry.day_count >= limit as u32
        {
            return Err(RateLimitRejection::Rpd { limit });
        }

        entry.minute_count += 1;
        entry.day_count += 1;
        Ok(())
    }

    pub fn add_tokens(&self, _user_id: i64, _model: &str, _total_tokens: i64) {
        // Reserved for future cumulative token windows. Per-request token caps are
        // enforced before dispatch using the declared token budget in the request.
    }
}

impl Default for RateLimitCounters {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Axum middleware
// ---------------------------------------------------------------------------

/// Axum middleware placeholder for rate limit enforcement.
///
/// Rate limiting is currently done inside the provider handler
/// (after authentication and model resolution).
/// This middleware is a pass-through reserved for future use.
pub async fn rate_limit_middleware(
    State(_state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    next.run(request).await
}

// ---------------------------------------------------------------------------
// Logic (called by AppState convenience methods)
// ---------------------------------------------------------------------------

/// Find the most specific matching rule. Priority: exact > prefix wildcard > `*`.
pub fn find_matching_rule<'a>(
    rules: &'a [RateLimitRule],
    model: &str,
) -> Option<&'a RateLimitRule> {
    if let Some(r) = rules.iter().find(|r| r.model_pattern == model) {
        return Some(r);
    }
    let mut best: Option<&RateLimitRule> = None;
    let mut best_len = 0;
    for rule in rules {
        if let Some(prefix) = rule.model_pattern.strip_suffix('*')
            && model.starts_with(prefix)
            && prefix.len() > best_len
        {
            best = Some(rule);
            best_len = prefix.len();
        }
    }
    if best.is_some() {
        return best;
    }
    rules.iter().find(|r| r.model_pattern == "*")
}
