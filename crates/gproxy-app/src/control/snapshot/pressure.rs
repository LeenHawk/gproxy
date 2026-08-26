use gproxy_core::Plan;
use rust_decimal::Decimal;

use super::types::{CredentialPressure, CredentialPressureMap};

pub(super) fn apply(plan: &mut Plan, pressure: &CredentialPressureMap, now: i64) {
    plan.targets
        .sort_by_key(|target| (target.tier, tier(pressure.get(&target.credential), now)));
}

fn tier(pressure: Option<&std::collections::BTreeMap<String, CredentialPressure>>, now: i64) -> u8 {
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
