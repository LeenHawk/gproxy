//! Ordering the candidates of one resolution.
//!
//! Ported from v3's `control/snapshot/balance.rs`, with two simplifications
//! the v4 shapes allow: a candidate is one route member rather than one
//! member/credential pair, so no de-duplication by member is needed, and
//! health is a rank this crate computes from credential blocks rather than a
//! separately persisted health map.
//!
//! The preference order is `(tier, health, Reverse(weight), id)`. Tier is a
//! hard preference: a later tier is never balanced against an earlier one.
//! Within the leading run — the candidates sharing the first one's tier and
//! health — the route's strategy decides which comes first.

use std::{
    cmp::Reverse,
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use gproxy_store::entity::routing::route::RouteStrategy;

use super::Target;

/// A candidate plus the health rank ordering uses. Rank 0 has at least one
/// credential that is usable right now; rank 1 has none that is (every one of
/// them is transiently blocked) but is still worth trying after the healthy
/// ones. Dead, retired and disabled credentials never reach this point.
pub(crate) struct Ranked {
    pub target: Target,
    pub health: u8,
}

impl Ranked {
    /// The stable tiebreak: the route member's id when there is one, the
    /// provider's otherwise, so two candidates never compare equal.
    fn id(&self) -> &str {
        self.target
            .member_id
            .as_deref()
            .unwrap_or(&self.target.provider.entity.id)
    }
}

/// Process-local rotation state, one entry per balance key. Losing it only
/// restarts rotation at the first candidate, so it is deliberately not shared
/// between instances: a counter in the cache would cost a round trip per
/// resolution to decide something that does not have to be globally fair.
#[derive(Default)]
pub struct RotationCounters {
    rotations: Mutex<HashMap<String, u64>>,
    weighted: Mutex<HashMap<String, WeightedPool>>,
}

#[derive(Default)]
struct WeightedPool {
    entries: Vec<(String, u32)>,
    current: Vec<i64>,
}

impl RotationCounters {
    /// The next rotation offset for this key. A poisoned lock is recovered
    /// from rather than propagated: rotation state is an optimization, and
    /// refusing to route because a counter panicked would be worse than
    /// starting the rotation over.
    fn next(&self, key: &str) -> u64 {
        let mut counters = recover(self.rotations.lock());
        let value = counters.entry(key.to_owned()).or_default();
        let current = *value;
        *value = value.wrapping_add(1);
        current
    }

    /// Smooth weighted round robin: each call adds every entry's weight to its
    /// current credit, picks the largest, and charges it the total. Over a
    /// full cycle each entry is picked in proportion to its weight, and the
    /// picks are spread out rather than batched.
    fn smooth(&self, key: &str, entries: &[(String, u32)]) -> String {
        let mut pools = recover(self.weighted.lock());
        let pool = pools.entry(key.to_owned()).or_default();
        // The candidate set changed (a reload, a blocked provider): the old
        // credits describe a different pool and would skew the first picks.
        if pool.entries != entries {
            pool.entries = entries.to_vec();
            pool.current = vec![0; entries.len()];
        }
        let mut total = 0i64;
        let mut selected = 0usize;
        for (index, (_, weight)) in entries.iter().enumerate() {
            let weight = i64::from(*weight);
            total += weight;
            pool.current[index] += weight;
            if pool.current[index] > pool.current[selected] {
                selected = index;
            }
        }
        pool.current[selected] -= total;
        entries[selected].0.clone()
    }
}

fn recover<'a, T>(
    result: Result<MutexGuard<'a, T>, std::sync::PoisonError<MutexGuard<'a, T>>>,
) -> MutexGuard<'a, T> {
    result.unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Order the candidates and return them as the plan's target list.
///
/// `key` identifies the rotation pool: a route id for an exposed name, a
/// synthetic key for the prefix and no-model forms, so two routes over the
/// same providers rotate independently.
pub(crate) fn order(
    mut ranked: Vec<Ranked>,
    strategy: RouteStrategy,
    key: &str,
    counters: &RotationCounters,
) -> Vec<Target> {
    ranked.sort_by(|a, b| {
        (a.target.tier, a.health, Reverse(a.target.weight), a.id()).cmp(&(
            b.target.tier,
            b.health,
            Reverse(b.target.weight),
            b.id(),
        ))
    });
    let Some(head) = ranked.first() else {
        return Vec::new();
    };
    // Only the leading run is balanced. Everything behind it is fallback and
    // keeps the deterministic order, which is what makes a later tier a
    // fallback rather than a share of the traffic.
    let (tier, health) = (head.target.tier, head.health);
    let length = ranked
        .iter()
        .position(|candidate| candidate.target.tier != tier || candidate.health != health)
        .unwrap_or(ranked.len());
    let mut fallback = ranked.split_off(length);
    let mut run = ranked;
    match strategy {
        RouteStrategy::RoundRobin if run.len() > 1 => {
            let offset = usize::try_from(counters.next(key) % run.len() as u64).unwrap_or(0);
            run.rotate_left(offset);
        }
        RouteStrategy::Weighted if run.len() > 1 => {
            let entries: Vec<(String, u32)> = run
                .iter()
                .map(|candidate| (candidate.id().to_owned(), candidate.target.weight))
                .collect();
            let selected = counters.smooth(key, &entries);
            // Only the winner moves; the rest keep their preference order, so
            // a weighted route still fails over predictably.
            if let Some(index) = run.iter().position(|candidate| candidate.id() == selected) {
                let winner = run.remove(index);
                run.insert(0, winner);
            }
        }
        // Failover, and any run of one, keep the sorted order.
        _ => {}
    }
    run.append(&mut fallback);
    run.into_iter().map(|candidate| candidate.target).collect()
}

/// A stable slot for a session on one provider: the same session keeps the
/// same credential across processes and restarts, and two sessions spread
/// across the credential set. splitmix64's finalizer over an FNV-1a hash of
/// both inputs, which is v3's `stable_slot` with string ids.
pub(crate) fn stable_slot(affinity_key: &str, provider_id: &str) -> u64 {
    let mut value = fnv1a(affinity_key) ^ fnv1a(provider_id).rotate_left(32);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value.wrapping_mul(0x94d0_49bb_1331_11eb) ^ (value >> 31)
}

fn fnv1a(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
