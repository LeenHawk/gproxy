//! Configured rate limits, enforced against the cache.
//!
//! A `rate_limits` row is a limit for one subject (a user or one of their
//! keys) over one model glob, expressed as a `metric` allowance per
//! `period_seconds`. The rows live in the snapshot; the live counters live in
//! the cache, because several instances share one limit and a per-process
//! counter would multiply every limit by the number of instances.
//!
//! Two shapes, two keys: a counter is per window, a permit is not.
//!
//! - **`concurrency`** takes a cache permit and holds it for the life of the
//!   request: it measures requests in flight.
//! - **every other metric** increments a counter with the limit as its
//!   ceiling: it measures requests made in the current window.
//!
//! Non-concurrency metrics are charged exactly 1 per request, whatever the
//! metric is named. A token count is not knowable at admission — it exists
//! only after the upstream has answered — so a limit that wants to cap tokens
//! is a *budget*, a `quotas` row core settles after the exchange, not a rate
//! limit. Enforcing a token metric as a request count here is the honest
//! approximation; inventing an estimate and charging it would produce a
//! number nothing later reconciles.
//!
//! **A cache failure is a refusal.** `design/cache.md` is explicit that a
//! failing cache must not fall back to local state, because instances would
//! then hand out contradictory quotas and locks; the same reasoning applies
//! one level up. Letting a request through because the counter could not be
//! read turns a cache outage into "every limit on the instance is off", which
//! is the one moment an attacker wants and an operator cannot see. The
//! rejection carries no retry hint: nothing here knows when the cache is back.

use crate::{AppData, AppError, Caller, snapshot::glob};
use gproxy_cache::{Cache, CacheError, IncrementOutcome, Version};
use gproxy_seaorm::FixedDecimal;
use gproxy_store::entity::limits::rate_limit;
use std::{sync::Arc, time::Duration};

/// The one metric that takes a permit instead of a counter. Compared
/// case-insensitively; every other value is a per-window request count.
pub const CONCURRENCY_METRIC: &str = "concurrency";

/// One charge taken from the cache, outstanding until it is given back.
///
/// Held in [`Admitted::rate_limit_leases`](super::Admitted) for the life of
/// the request. Dropping it returns the charge; see [`RateLease::finish`] for
/// the one case where that is not what should happen.
pub struct RateLease {
    cache: Arc<dyn Cache>,
    /// The `rate_limits` row this came from, for the operator's log.
    limit_id: String,
    key: String,
    charge: Charge,
    /// Set once the charge has been given back, or once `finish` decided it
    /// should stand. Both paths must be idempotent: a counter released twice
    /// would let the next request in for free.
    settled: bool,
}

#[derive(Clone, Copy)]
enum Charge {
    Permit { owner: Version },
    Counter { generation: Version, amount: u64 },
}

impl RateLease {
    /// The `rate_limits` row behind this charge.
    pub fn limit_id(&self) -> &str {
        &self.limit_id
    }

    /// The cache key the charge is held under, which encodes the row and the
    /// window it belongs to.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Whether this is a concurrency permit rather than a window counter.
    pub fn is_permit(&self) -> bool {
        matches!(self.charge, Charge::Permit { .. })
    }

    /// The request was admitted and ran.
    ///
    /// A **window counter** records that a request was made, so it stands: it
    /// is not refunded when the lease drops, and calling this twice does not
    /// refund it twice. A **concurrency permit** measures requests in flight,
    /// so it is still returned on drop — there is nothing about having
    /// finished successfully that should keep a slot occupied.
    pub fn finish(&mut self) {
        if matches!(self.charge, Charge::Counter { .. }) {
            self.settled = true;
        }
    }

    /// Give the charge back now, awaiting the cache.
    ///
    /// This is the path admission itself takes when a later row in the same
    /// request rejects, and the one a host should take at the end of a
    /// request: it reports nothing, but it has actually completed by the time
    /// it returns, unlike the drop path.
    pub async fn release(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        give_back(
            self.cache.clone(),
            self.key.clone(),
            self.charge,
            self.limit_id.clone(),
        )
        .await;
    }
}

impl std::fmt::Debug for RateLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLease")
            .field("limit_id", &self.limit_id)
            .field("key", &self.key)
            .field("permit", &self.is_permit())
            .field("settled", &self.settled)
            .finish()
    }
}

/// Returning a charge is an async cache call and `Drop` cannot await one, so
/// it is spawned. Without a runtime to spawn onto there is nothing left to do
/// but say so: every charge has a TTL that ends with its window, so the worst
/// case is a slot or a count held until the window rolls over, never forever.
impl Drop for RateLease {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        let (cache, key, charge, limit_id) = (
            self.cache.clone(),
            self.key.clone(),
            self.charge,
            self.limit_id.clone(),
        );
        let logged = limit_id.clone();
        if !crate::rt::spawn(async move { give_back(cache, key, charge, limit_id).await }) {
            tracing::warn!(
                limit = %logged,
                key = %self.key,
                "no runtime to return a rate-limit charge on; it is released when its window ends"
            );
        }
    }
}

async fn give_back(cache: Arc<dyn Cache>, key: String, charge: Charge, limit_id: String) {
    let outcome = match charge {
        Charge::Permit { owner } => cache.release_permit(&key, owner).await.map(|_| ()),
        Charge::Counter { generation, amount } => {
            cache.decrement(&key, generation, amount).await.map(|_| ())
        }
    };
    if let Err(error) = outcome {
        tracing::warn!(
            %error,
            limit = %limit_id,
            key = %key,
            "could not return a rate-limit charge; it is released when its window ends"
        );
    }
}

/// Charge every applicable limit, or reject.
///
/// Rows are taken in snapshot order and each one is charged before the next is
/// looked at, so the first rejection stops the walk. Everything already
/// charged in this request is given back before the error is returned:
/// a request that never ran must not consume any of the allowances it was
/// refused by.
///
/// Called **last** in admission. A request refused for permissions, or one
/// whose caller may reach no provider at all, must not move a counter — the
/// alternative lets an unauthorized client exhaust a legitimate caller's
/// window by sending requests it was never going to be allowed to make.
pub async fn apply(
    snapshot: &AppData,
    cache: &Arc<dyn Cache>,
    caller: &Caller,
    model: Option<&str>,
    now_ms: i64,
) -> Result<Vec<RateLease>, AppError> {
    apply_selected(snapshot, cache, caller, model, now_ms, true, true).await
}

pub(crate) async fn apply_selected(
    snapshot: &AppData,
    cache: &Arc<dyn Cache>,
    caller: &Caller,
    model: Option<&str>,
    now_ms: i64,
    counters: bool,
    permits: bool,
) -> Result<Vec<RateLease>, AppError> {
    let mut held: Vec<RateLease> = Vec::new();
    for row in &snapshot.rate_limits {
        if !applies(row, caller, model) {
            continue;
        }
        if if row.metric.eq_ignore_ascii_case(CONCURRENCY_METRIC) {
            !permits
        } else {
            !counters
        } {
            continue;
        }
        match charge(cache, row, now_ms).await {
            Ok(Some(lease)) => held.push(lease),
            // An unusable row: skipped and logged, never enforced as zero.
            Ok(None) => {}
            Err(error) => {
                for lease in &mut held {
                    lease.release().await;
                }
                return Err(error);
            }
        }
    }
    Ok(held)
}

/// Whether one row covers this request.
///
/// The subject columns follow the same rule as a permission rule: every
/// column that is set must match, so a row naming both a user and a key is the
/// intersection of the two. A row with **neither** set applies to nobody and
/// is dropped rather than read as a limit on everybody — an instance-wide
/// limit that appeared because a column was forgotten would be indefensible.
fn applies(row: &rate_limit::Model, caller: &Caller, model: Option<&str>) -> bool {
    if !row.enabled {
        return false;
    }
    if row.user_id.is_none() && row.api_key_id.is_none() {
        return false;
    }
    if let Some(user_id) = &row.user_id
        && user_id != &caller.user_id
    {
        return false;
    }
    if let Some(api_key_id) = &row.api_key_id
        && caller.api_key_id.as_deref() != Some(api_key_id.as_str())
    {
        return false;
    }
    match row
        .model_pattern
        .as_deref()
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty() && *pattern != "*")
    {
        // Blank, absent or `*`: every model, and a request that names none.
        None => true,
        // A narrower pattern cannot describe a request without a model.
        Some(pattern) => model.is_some_and(|model| glob::matches(pattern, model)),
    }
}

/// Take one row's charge. `Ok(None)` means the row could not be enforced and
/// was skipped; an error means the request is refused.
async fn charge(
    cache: &Arc<dyn Cache>,
    row: &rate_limit::Model,
    now_ms: i64,
) -> Result<Option<RateLease>, AppError> {
    let Some(window) = Window::of(row.period_seconds, now_ms) else {
        tracing::warn!(
            limit = %row.id,
            period_seconds = row.period_seconds,
            "rate limit skipped: its period is not a positive number of seconds"
        );
        return Ok(None);
    };
    let concurrency = row.metric.trim().eq_ignore_ascii_case(CONCURRENCY_METRIC);
    // A counter belongs to its window and must vanish when the window rolls
    // over. A permit measures requests in flight *now*, which no window
    // divides: keying it by window would let a request started before a
    // boundary and one started after it hold a slot each, admitting twice the
    // limit for as long as the first is running.
    let key = if concurrency {
        format!("rl/{}/live", row.id)
    } else {
        format!("rl/{}/{}", row.id, window.start_ms)
    };
    let limit = limit_of(row);
    // For a permit the TTL is only the reclaim bound for a request that dies
    // without releasing, so it is the whole period rather than what is left
    // of the current one.
    let ttl = if concurrency {
        window.span()
    } else {
        window.remaining(now_ms)
    };
    let rejected = || AppError::RateLimited {
        retry_after_ms: Some(window.end_ms - now_ms),
    };
    // A limit of zero forbids everything. It is answered here rather than by
    // the cache, which rejects a permit limit of zero as an invalid argument —
    // and an invalid argument is an error, which would be indistinguishable
    // from the cache being down.
    if limit == 0 {
        return Err(rejected());
    }
    let charge = if concurrency {
        // A concurrency limit above the cache's `max_permits_per_key` (10 000
        // by default) is not representable; the cache refuses it, and the
        // fail-closed rule below then refuses every request. Keep such limits
        // within range.
        let permits = u32::try_from(limit).unwrap_or(u32::MAX);
        match cache.acquire_permit(&key, permits, ttl).await {
            Ok(Some(owner)) => Charge::Permit { owner },
            Ok(None) => return Err(rejected()),
            Err(error) => return Err(unavailable(row, &key, error)),
        }
    } else {
        match cache.increment(&key, 1, limit, ttl).await {
            Ok(IncrementOutcome::Applied(counter)) => Charge::Counter {
                generation: counter.generation,
                amount: 1,
            },
            Ok(IncrementOutcome::Limited { .. }) => return Err(rejected()),
            Err(error) => return Err(unavailable(row, &key, error)),
        }
    };
    Ok(Some(RateLease {
        cache: cache.clone(),
        limit_id: row.id.clone(),
        key,
        charge,
        settled: false,
    }))
}

/// A cache that could not answer refuses the request. See the module header.
fn unavailable(row: &rate_limit::Model, key: &str, error: CacheError) -> AppError {
    tracing::warn!(
        %error,
        limit = %row.id,
        key,
        "rate limit could not be evaluated; refusing the request rather than passing it"
    );
    AppError::RateLimited {
        retry_after_ms: None,
    }
}

/// The window a row is in at `now_ms`.
///
/// Windows are fixed and aligned to the epoch rather than to the first
/// request, so every instance computes the same boundary from the clock alone
/// and no coordination is needed to agree on which window a request falls in.
struct Window {
    start_ms: i64,
    end_ms: i64,
}

impl Window {
    fn of(period_seconds: i64, now_ms: i64) -> Option<Self> {
        let span_ms = period_seconds.checked_mul(1_000).filter(|ms| *ms > 0)?;
        // `rem_euclid` rather than `%`: a clock before the epoch would give a
        // negative remainder and a window start in the future.
        let start_ms = now_ms - now_ms.rem_euclid(span_ms);
        Some(Self {
            start_ms,
            end_ms: start_ms.saturating_add(span_ms),
        })
    }

    /// The whole period: how long a permit may be held before the cache
    /// reclaims it from a request that died without releasing. A concurrency
    /// limit's period is therefore the longest request it expects to guard.
    fn span(&self) -> Duration {
        Duration::from_millis((self.end_ms - self.start_ms).max(1) as u64)
    }

    /// What is left of the window, which is how long a counter lives: the
    /// count must vanish when the window rolls over.
    fn remaining(&self, now_ms: i64) -> Duration {
        let ms = (self.end_ms - now_ms).max(1);
        Duration::from_millis(ms as u64)
    }
}

/// `limit_value` shares the fixed-point column shape of the money limits, but
/// a rate limit is a whole count: the fractional part is truncated and a
/// negative value reads as zero, which forbids everything.
fn limit_of(row: &rate_limit::Model) -> u64 {
    let atoms = row.limit_value.atoms().max(0);
    u64::try_from(atoms / FixedDecimal::FACTOR).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::support::caller;
    use gproxy_cache::MemoryCache;
    use gproxy_store::IdentityData;

    const NOW: i64 = 1_700_000_000_000;

    fn row(id: &str, metric: &str, limit: i64, period_seconds: i64) -> rate_limit::Model {
        rate_limit::Model {
            id: id.into(),
            user_id: Some("alice".into()),
            api_key_id: None,
            metric: metric.into(),
            limit_value: FixedDecimal::from_atoms(limit * FixedDecimal::FACTOR),
            period_seconds,
            model_pattern: None,
            enabled: true,
        }
    }

    fn snapshot(rows: Vec<rate_limit::Model>) -> AppData {
        let identity = IdentityData {
            rate_limits: rows,
            ..IdentityData::default()
        };
        AppData::assemble_at(1, &identity, &[], NOW).unwrap()
    }

    fn cache() -> Arc<dyn Cache> {
        Arc::new(MemoryCache::new(Default::default()).unwrap())
    }

    #[tokio::test]
    async fn a_request_under_the_ceiling_passes_and_holds_a_charge() {
        let data = snapshot(vec![row("r", "requests", 2, 60)]);
        let cache = cache();
        let leases = apply(&data, &cache, &caller("alice", "user"), None, NOW)
            .await
            .unwrap();
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].limit_id(), "r");
        assert!(!leases[0].is_permit());
        // The window is aligned to the epoch, not to this request.
        assert_eq!(leases[0].key(), "rl/r/1699999980000");
    }

    #[tokio::test]
    async fn the_request_at_the_ceiling_is_rejected_with_a_retry_hint() {
        let data = snapshot(vec![row("r", "requests", 2, 60)]);
        let cache = cache();
        let caller = caller("alice", "user");
        for _ in 0..2 {
            let mut leases = apply(&data, &cache, &caller, None, NOW).await.unwrap();
            // The request went through: the count stands.
            for lease in &mut leases {
                lease.finish();
            }
        }
        let error = apply(&data, &cache, &caller, None, NOW).await.unwrap_err();
        assert_eq!(error.status_code(), 429);
        // 1 699 999 980 000 + 60 000 − NOW.
        assert!(
            matches!(error, AppError::RateLimited { retry_after_ms: Some(ms) } if ms == 40_000),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_new_window_starts_from_zero() {
        let data = snapshot(vec![row("r", "requests", 1, 60)]);
        let cache = cache();
        let caller = caller("alice", "user");
        let mut leases = apply(&data, &cache, &caller, None, NOW).await.unwrap();
        leases.iter_mut().for_each(RateLease::finish);
        assert!(apply(&data, &cache, &caller, None, NOW).await.is_err());
        // One whole period later is a different key, hence a fresh count.
        let later = apply(&data, &cache, &caller, None, NOW + 60_000)
            .await
            .unwrap();
        assert_eq!(later[0].key(), "rl/r/1700000040000");
    }

    #[tokio::test]
    async fn a_counter_is_refunded_when_the_request_never_ran() {
        let data = snapshot(vec![row("r", "requests", 1, 60)]);
        let cache = cache();
        let caller = caller("alice", "user");
        // Charged and dropped without `finish`: the request did not happen.
        drop(apply(&data, &cache, &caller, None, NOW).await.unwrap());
        settle().await;
        assert!(apply(&data, &cache, &caller, None, NOW).await.is_ok());
    }

    #[tokio::test]
    async fn a_concurrency_permit_is_released_when_its_lease_drops() {
        let data = snapshot(vec![row("r", "concurrency", 1, 60)]);
        let cache = cache();
        let caller = caller("alice", "user");
        let mut held = apply(&data, &cache, &caller, None, NOW).await.unwrap();
        assert!(held[0].is_permit());
        // The slot is taken while the lease lives.
        assert!(apply(&data, &cache, &caller, None, NOW).await.is_err());
        // `finish` does not keep a slot occupied: it is in-flight, not made.
        held.iter_mut().for_each(RateLease::finish);
        drop(held);
        settle().await;
        assert!(apply(&data, &cache, &caller, None, NOW).await.is_ok());
    }

    #[tokio::test]
    async fn a_row_that_rejects_gives_back_what_an_earlier_row_took() {
        let data = snapshot(vec![
            row("wide", "requests", 10, 60),
            row("narrow", "requests", 0, 60),
        ]);
        let cache = cache();
        let caller = caller("alice", "user");
        assert!(apply(&data, &cache, &caller, None, NOW).await.is_err());
        settle().await;
        // `wide` was charged and returned, so its count is still zero: ten
        // refusals by `narrow` must not exhaust it.
        let counter = cache.counter("rl/wide/1699999980000").await.unwrap();
        assert!(
            counter.is_none_or(|counter| counter.value == 0),
            "{counter:?}"
        );
    }

    #[tokio::test]
    async fn a_failing_cache_refuses_rather_than_passes() {
        let data = snapshot(vec![row("r", "requests", 10, 60)]);
        let cache: Arc<dyn Cache> = Arc::new(BrokenCache);
        let error = apply(&data, &cache, &caller("alice", "user"), None, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 429);
        assert!(matches!(
            error,
            AppError::RateLimited {
                retry_after_ms: None
            }
        ));
    }

    #[tokio::test]
    async fn only_the_rows_that_cover_the_request_are_charged() {
        let mut other_user = row("other-user", "requests", 1, 60);
        other_user.user_id = Some("bob".into());
        let mut other_key = row("other-key", "requests", 1, 60);
        other_key.api_key_id = Some("k2".into());
        let mut this_key = row("this-key", "requests", 1, 60);
        this_key.api_key_id = Some("k1".into());
        let mut narrow_model = row("narrow-model", "requests", 1, 60);
        narrow_model.model_pattern = Some("claude-*".into());
        let mut wide_model = row("wide-model", "requests", 1, 60);
        wide_model.model_pattern = Some("gpt-*".into());
        let mut disabled = row("disabled", "requests", 1, 60);
        disabled.enabled = false;
        let mut nobody = row("nobody", "requests", 1, 60);
        nobody.user_id = None;
        let data = snapshot(vec![
            other_user,
            other_key,
            this_key,
            narrow_model,
            wide_model,
            disabled,
            nobody,
            row("everything", "requests", 1, 60),
        ]);
        let leases = apply(
            &data,
            &cache(),
            &caller("alice", "user"),
            Some("gpt-4o"),
            NOW,
        )
        .await
        .unwrap();
        let charged: Vec<&str> = leases.iter().map(RateLease::limit_id).collect();
        assert_eq!(charged, ["this-key", "wide-model", "everything"]);
    }

    #[tokio::test]
    async fn a_blank_or_star_pattern_also_covers_a_request_with_no_model() {
        for pattern in [None, Some(""), Some("  "), Some("*")] {
            let mut model_any = row("r", "requests", 1, 60);
            model_any.model_pattern = pattern.map(Into::into);
            let data = snapshot(vec![model_any]);
            let leases = apply(&data, &cache(), &caller("alice", "user"), None, NOW)
                .await
                .unwrap();
            assert_eq!(leases.len(), 1, "{pattern:?}");
        }
        // A narrower one cannot.
        let mut narrow = row("r", "requests", 1, 60);
        narrow.model_pattern = Some("gpt-*".into());
        let data = snapshot(vec![narrow]);
        assert!(
            apply(&data, &cache(), &caller("alice", "user"), None, NOW)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_row_with_an_unusable_period_is_skipped_not_enforced() {
        for period in [0, -60] {
            let data = snapshot(vec![row("r", "requests", 1, period)]);
            let leases = apply(&data, &cache(), &caller("alice", "user"), None, NOW)
                .await
                .unwrap();
            assert!(leases.is_empty(), "{period}");
        }
    }

    #[test]
    fn a_limit_is_a_whole_count_and_never_negative() {
        let mut fractional = row("r", "requests", 0, 60);
        fractional.limit_value = FixedDecimal::from_atoms(1_500_000_000);
        assert_eq!(limit_of(&fractional), 1);
        let mut negative = row("r", "requests", 0, 60);
        negative.limit_value = FixedDecimal::from_atoms(-5 * FixedDecimal::FACTOR);
        assert_eq!(limit_of(&negative), 0);
    }

    /// The drop path spawns its release, so a test that observes the effect
    /// has to let the runtime run it.
    async fn settle() {
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
    }

    /// Every operation fails, which is what an unreachable Redis looks like.
    struct BrokenCache;

    #[async_trait::async_trait]
    impl Cache for BrokenCache {
        async fn get(&self, _key: &str) -> gproxy_cache::Result<Option<gproxy_cache::Entry>> {
            Err(CacheError::Timeout)
        }
        async fn put(
            &self,
            _key: &str,
            _value: Vec<u8>,
            _ttl: Duration,
        ) -> gproxy_cache::Result<Version> {
            Err(CacheError::Timeout)
        }
        async fn delete(&self, _key: &str) -> gproxy_cache::Result<bool> {
            Err(CacheError::Timeout)
        }
        async fn compare_exchange(
            &self,
            _key: &str,
            _expected: Option<Version>,
            _replacement: Option<gproxy_cache::Replacement>,
        ) -> gproxy_cache::Result<gproxy_cache::CasOutcome> {
            Err(CacheError::Timeout)
        }
        async fn counter(&self, _key: &str) -> gproxy_cache::Result<Option<gproxy_cache::Counter>> {
            Err(CacheError::Timeout)
        }
        async fn increment(
            &self,
            _key: &str,
            _amount: u64,
            _limit: u64,
            _ttl: Duration,
        ) -> gproxy_cache::Result<IncrementOutcome> {
            Err(CacheError::Timeout)
        }
        async fn decrement(
            &self,
            _key: &str,
            _generation: Version,
            _amount: u64,
        ) -> gproxy_cache::Result<Option<gproxy_cache::Counter>> {
            Err(CacheError::Timeout)
        }
        async fn acquire_permit(
            &self,
            _key: &str,
            _limit: u32,
            _ttl: Duration,
        ) -> gproxy_cache::Result<Option<Version>> {
            Err(CacheError::Timeout)
        }
        async fn renew_permit(
            &self,
            _key: &str,
            _owner: Version,
            _ttl: Duration,
        ) -> gproxy_cache::Result<bool> {
            Err(CacheError::Timeout)
        }
        async fn release_permit(&self, _key: &str, _owner: Version) -> gproxy_cache::Result<bool> {
            Err(CacheError::Timeout)
        }
        async fn publish(&self, _topic: &str, _payload: Vec<u8>) -> gproxy_cache::Result<()> {
            Err(CacheError::Timeout)
        }
        async fn subscribe(
            &self,
            _topic: &str,
        ) -> gproxy_cache::Result<Box<dyn gproxy_cache::Subscription>> {
            Err(CacheError::Timeout)
        }
    }
}
