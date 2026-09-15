use gproxy_core::Plan;
use rust_decimal::Decimal;

use std::collections::BTreeMap;

use super::types::{CredentialPressure, CredentialPressureMap, CredentialStrategy};

pub(super) fn apply(
    plan: &mut Plan,
    pressure: &CredentialPressureMap,
    strategies: &BTreeMap<i64, CredentialStrategy>,
    now: i64,
) {
    plan.targets.sort_by_key(|target| {
        let windows = pressure.get(&target.credential);
        let rank =
            if strategies.get(&target.provider.id) == Some(&CredentialStrategy::EarliestReset) {
                // This opt-in strategy deliberately spends the last usable quota,
                // even at 90%+. Keep exhausted accounts as last-resort fallbacks.
                u8::from(earliest_reset(windows, &target.upstream_model, now).0 == 2) * 2
            } else {
                tier(windows, now)
            };
        (target.tier, rank)
    });
}

/// Rank only live windows applicable to this upstream model. Known usable
/// resets precede unknown dates; any exhausted applicable window wins over a
/// usable one (e.g. exhausted weekly quota with a fresh five-hour window).
/// Unknown and expired observations never fabricate a new reset deadline.
pub(super) fn earliest_reset(
    pressure: Option<&BTreeMap<String, CredentialPressure>>,
    model: &str,
    now: i64,
) -> (u8, i64) {
    let mut earliest = None;
    for window in pressure.into_iter().flat_map(|windows| windows.values()) {
        if !window.scope.includes(model) || window.period_end.is_some_and(|end| end <= now) {
            continue;
        }
        if window.used_percent >= Decimal::ONE_HUNDRED {
            return (2, i64::MAX);
        }
        if let Some(end) = window.period_end {
            earliest = Some(earliest.map_or(end, |current: i64| current.min(end)));
        }
    }
    earliest.map_or((1, i64::MAX), |end| (0, end))
}

fn tier(pressure: Option<&BTreeMap<String, CredentialPressure>>, now: i64) -> u8 {
    let pressure = pressure
        .into_iter()
        .flat_map(|windows| windows.values())
        .filter(|window| window.period_end.is_none_or(|period_end| period_end > now))
        .map(|window| window.used_percent)
        .max();
    match pressure {
        Some(pressure) if pressure >= Decimal::from(100) => 2,
        Some(pressure) if pressure >= Decimal::from(90) => 1,
        _ => 0,
    }
}
