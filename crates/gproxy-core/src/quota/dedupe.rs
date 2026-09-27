//! Write dedupe for the quota observation log. Header observers report on
//! every upstream answer, so persisting each reading would put one row per
//! window per request on Store's single writer. An entry is persisted only
//! when it differs materially from the latest persisted reading of the same
//! `(credential, entry id)`, or a heartbeat has passed.
//!
//! The latest persisted readings live in a small per-credential cache
//! projection rather than a Store read: the snapshot id is inside JSON, so a
//! query could not select it without scanning the credential's history. The
//! projection is best-effort; a miss, a parse failure or a lost CAS only
//! means the next observation is written.

use super::{allowance, snapshot_json};
use crate::{CoreError, CoreResult, keys};
use gproxy_cache::{Cache, CasOutcome, Replacement};
use gproxy_channel::channel::{QuotaAllowance, QuotaDimension, QuotaEntry, QuotaWindow};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// An unchanged window is still written this often, so the log shows it
/// was alive and a rebuilt reset projection is never far behind.
pub(super) const HEARTBEAT_MS: i64 = 15 * 60 * 1000;
/// Boundary drift below this is the same period: upstreams round reset
/// times and derive them from their own clock.
pub(super) const PERIOD_TOLERANCE_MS: i64 = 5 * 60 * 1000;

/// The latest persisted reading of one entry.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Persisted {
    id: String,
    observed_at_ms: i64,
    /// The row's snapshot and scope without period boundaries; any
    /// difference here is material.
    value: serde_json::Value,
    starts_at_ms: Option<i64>,
    resets_at_ms: Option<i64>,
    /// An unused rolling window whose reset floats with the observation time.
    not_started: bool,
}

impl Persisted {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn new(dimensions: &[QuotaDimension], entry: &QuotaEntry, now_ms: i64) -> Self {
        let mut value = snapshot_json(entry);
        if let Some(object) = value.as_object_mut() {
            object.remove("period_start_ms");
            object.remove("period_end_ms");
            object.insert(
                "scope".into(),
                serde_json::to_value(&entry.model_scope).unwrap_or_default(),
            );
        }
        let period = allowance(&entry.value);
        Self {
            id: entry.id.clone(),
            observed_at_ms: now_ms,
            value,
            starts_at_ms: period.and_then(|a| a.period_start_ms),
            resets_at_ms: period.and_then(|a| a.period_end_ms),
            not_started: not_started(dimensions, entry, now_ms),
        }
    }

    /// Whether `next` must be persisted given this latest persisted reading.
    pub(super) fn needs_write(&self, next: &Self) -> bool {
        if next.observed_at_ms.saturating_sub(self.observed_at_ms) >= HEARTBEAT_MS
            || self.value != next.value
        {
            return true;
        }
        // Both readings describe an unused window: its reset is "now plus
        // the window" each time, which is not a new period.
        if self.not_started && next.not_started {
            return false;
        }
        moved(self.starts_at_ms, next.starts_at_ms) || moved(self.resets_at_ms, next.resets_at_ms)
    }
}

fn moved(before: Option<i64>, after: Option<i64>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => before.abs_diff(after) > PERIOD_TOLERANCE_MS as u64,
        (None, None) => false,
        _ => true,
    }
}

/// A rolling window nobody used reports zero usage and resets one window
/// length after the observation. The length is the reported period when
/// both ends are known, else the declared rolling dimension's.
fn not_started(dimensions: &[QuotaDimension], entry: &QuotaEntry, now_ms: i64) -> bool {
    let Some(a) = allowance(&entry.value) else {
        return false;
    };
    let window_ms = match (a.period_start_ms, a.period_end_ms) {
        (Some(start), Some(end)) => Some(end.saturating_sub(start)),
        _ => dimensions
            .iter()
            .find(|d| d.id == entry.source_id)
            .and_then(|d| match d.window {
                QuotaWindow::Rolling { seconds } => Some(seconds.saturating_mul(1000)),
                _ => None,
            }),
    };
    idle_window(a, window_ms, now_ms)
}

/// Whether `a` reads as an unused window of `window_ms`: nothing used, and a
/// reset one window after `now_ms` within the tolerance.
pub(super) fn idle_window(a: &QuotaAllowance, window_ms: Option<i64>, now_ms: i64) -> bool {
    let idle = match a.used_percent {
        Some(percent) => percent.is_zero(),
        None => a.used.is_some_and(|used| used.is_zero()),
    };
    let Some(end) = a.period_end_ms.filter(|_| idle) else {
        return false;
    };
    window_ms.is_some_and(|window| {
        window > 0 && end.abs_diff(now_ms.saturating_add(window)) <= PERIOD_TOLERANCE_MS as u64
    })
}

/// The latest persisted readings of a credential; empty on a cold or
/// unreadable projection, which makes every entry a write.
pub(super) async fn persisted_quota_observations(
    cache: &dyn Cache,
    credential_id: &str,
) -> CoreResult<Vec<Persisted>> {
    let key = keys::credential_quota_observations(credential_id);
    Ok(cache
        .get(&key)
        .await?
        .and_then(|entry| serde_json::from_slice(&entry.value).ok())
        .unwrap_or_default())
}

/// Merge the readings just persisted into the projection. CAS keeps a
/// concurrent answer's other windows; readings past the heartbeat are
/// dropped because they could no longer suppress a write.
pub(super) async fn record_quota_observations(
    cache: &dyn Cache,
    credential_id: &str,
    written: Vec<Persisted>,
    now_ms: i64,
) -> CoreResult<()> {
    let key = keys::credential_quota_observations(credential_id);
    for _ in 0..3 {
        let cached = cache.get(&key).await?;
        let mut rows: Vec<Persisted> = cached
            .as_ref()
            .and_then(|entry| serde_json::from_slice(&entry.value).ok())
            .unwrap_or_default();
        for next in &written {
            if rows
                .iter()
                .any(|r| r.id == next.id && r.observed_at_ms > next.observed_at_ms)
            {
                continue;
            }
            rows.retain(|r| r.id != next.id);
            rows.push(next.clone());
        }
        rows.retain(|r| now_ms.saturating_sub(r.observed_at_ms) < HEARTBEAT_MS);
        let replacement = Replacement {
            value: serde_json::to_vec(&rows).map_err(|e| CoreError::Rewrite(e.to_string()))?,
            ttl: Duration::from_millis(HEARTBEAT_MS as u64),
        };
        if matches!(
            cache
                .compare_exchange(&key, cached.map(|c| c.version), Some(replacement))
                .await?,
            CasOutcome::Applied(_)
        ) {
            return Ok(());
        }
    }
    // A contended projection is dropped; the next readings are written.
    cache.delete(&key).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_channel::channel::{QuotaAllowance, QuotaScope, QuotaSubject, QuotaValue};

    const WINDOW_MS: i64 = 5 * 60 * 60 * 1000;
    const NOW: i64 = 1_789_812_000_000;

    fn entry(percent: i64, start: Option<i64>, end: Option<i64>) -> QuotaEntry {
        QuotaEntry {
            id: "five_hour".into(),
            source_id: "five_hour".into(),
            label: None,
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Window(QuotaAllowance {
                used_percent: Some(percent.into()),
                period_start_ms: start,
                period_end_ms: end,
                ..Default::default()
            }),
        }
    }

    fn reading(percent: i64, end: Option<i64>, at: i64) -> Persisted {
        Persisted::new(&[], &entry(percent, None, end), at)
    }

    #[test]
    fn unchanged_readings_within_the_heartbeat_are_skipped() {
        let reset = NOW + 60 * 60 * 1000;
        let first = reading(40, Some(reset), NOW);
        assert!(!first.needs_write(&reading(40, Some(reset), NOW + 60_000)));
        // Reset drift inside the tolerance is the same period.
        assert!(!first.needs_write(&reading(40, Some(reset + PERIOD_TOLERANCE_MS), NOW + 1)));
    }

    #[test]
    fn material_changes_and_the_heartbeat_are_written() {
        let reset = NOW + 60 * 60 * 1000;
        let first = reading(40, Some(reset), NOW);
        assert!(first.needs_write(&reading(41, Some(reset), NOW + 1)));
        assert!(first.needs_write(&reading(40, Some(reset + PERIOD_TOLERANCE_MS + 1), NOW + 1)));
        assert!(first.needs_write(&reading(40, None, NOW + 1)));
        assert!(first.needs_write(&reading(40, Some(reset), NOW + HEARTBEAT_MS)));
        assert!(!first.needs_write(&reading(40, Some(reset), NOW + HEARTBEAT_MS - 1)));
    }

    #[test]
    fn a_floating_reset_of_an_unused_window_is_not_a_change() {
        let at = |now: i64, drift: i64| {
            let start = now + drift;
            Persisted::new(&[], &entry(0, Some(start), Some(start + WINDOW_MS)), now)
        };
        let first = at(NOW, -4 * 60 * 1000);
        let second = at(NOW + 60_000, 4 * 60 * 1000);
        assert!(first.not_started && second.not_started);
        assert!(
            !first.needs_write(&second),
            "8 minutes of float is not a period"
        );
        // Once used, the same drift is a new period.
        let used = Persisted::new(
            &[],
            &entry(1, second.starts_at_ms, second.resets_at_ms),
            NOW + 60_000,
        );
        assert!(!used.not_started);
        assert!(first.needs_write(&used));
        // A reset far from "now plus the window" is a started window.
        let started = Persisted::new(
            &[],
            &entry(0, Some(NOW - WINDOW_MS / 2), Some(NOW + WINDOW_MS / 2)),
            NOW,
        );
        assert!(!started.not_started);
    }
}
