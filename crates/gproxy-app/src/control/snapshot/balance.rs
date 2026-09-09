use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::types::{CredentialHealthMap, CredentialStrategy, TargetSeed};

mod rotation;
#[cfg(test)]
mod tests;

use gproxy_store::records::RouteStrategy;
pub(super) use rotation::RotationCounters;

pub(super) fn order(
    mut seeds: Vec<TargetSeed>,
    strategy: RouteStrategy,
    balance_key: i64,
    affinity: Option<i64>,
    health: &CredentialHealthMap,
    counters: &RotationCounters,
) -> Vec<TargetSeed> {
    seeds.retain(|seed| health_rank(seed, health) < 2);
    seeds.sort_by_key(|seed| {
        (
            seed.tier,
            health_rank(seed, health),
            Reverse(seed.member_weight),
            seed.member_id,
            Reverse(seed.credential_weight),
            seed.credential.0,
        )
    });
    let Some(primary_tier) = seeds.first().map(|seed| seed.tier) else {
        return seeds;
    };
    let primary_health = health_rank(&seeds[0], health);
    let primary_end = seeds
        .iter()
        .position(|seed| seed.tier != primary_tier || health_rank(seed, health) != primary_health)
        .unwrap_or(seeds.len());
    let mut members = Vec::new();
    for seed in &seeds[..primary_end] {
        if members.last().is_none_or(|(id, _)| *id != seed.member_id) {
            members.push((seed.member_id, seed.member_weight));
        }
    }
    match strategy {
        RouteStrategy::RoundRobin => {
            let start = (counters.next((0, balance_key, 0)) % members.len() as u64) as usize;
            members.rotate_left(start);
        }
        RouteStrategy::Weighted => {
            let member_id = counters.smooth((0, balance_key, 0), &members);
            let index = members
                .iter()
                .position(|(id, _)| *id == member_id)
                .expect("selected member");
            let selected = members.remove(index);
            members.insert(0, selected);
        }
        RouteStrategy::Failover => {}
    }
    let order = members
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (*id, index))
        .collect::<BTreeMap<_, _>>();
    seeds[..primary_end].sort_by_key(|seed| order[&seed.member_id]);
    let member_id = members[0].0;
    let credentials = seeds[..primary_end]
        .iter()
        .filter(|seed| seed.member_id == member_id)
        .map(|seed| (seed.credential.0, seed.credential_weight))
        .collect::<Vec<_>>();
    let strategy = seeds
        .iter()
        .find(|seed| seed.member_id == member_id)
        .map(|seed| seed.credential_strategy)
        .unwrap_or(CredentialStrategy::RoundRobin);
    let credential_id = match strategy {
        CredentialStrategy::RoundRobin => {
            counters.smooth((1, balance_key, member_id), &credentials)
        }
        CredentialStrategy::Sticky => weighted_owner(
            &credentials,
            affinity.map_or(0, |key| stable_slot(key, member_id)),
        ),
    };
    if let Some(index) = seeds
        .iter()
        .position(|seed| seed.member_id == member_id && seed.credential.0 == credential_id)
    {
        let selected = seeds.remove(index);
        seeds.insert(0, selected);
    }
    seeds
}

fn health_rank(seed: &TargetSeed, health: &CredentialHealthMap) -> u8 {
    ["*", seed.upstream_model.as_str()]
        .into_iter()
        .filter_map(|model| health.get(&seed.credential)?.get(model))
        .filter(|(version, _)| *version == seed.credential_version)
        .map(|(_, state)| match state {
            gproxy_store::records::CredentialHealthState::Healthy => 0,
            gproxy_store::records::CredentialHealthState::Degraded => 1,
            gproxy_store::records::CredentialHealthState::Dead => 2,
        })
        .max()
        .unwrap_or(0)
}

fn stable_slot(key: i64, provider_id: i64) -> u64 {
    let mut value = u64::from_ne_bytes(key.to_ne_bytes())
        ^ u64::from_ne_bytes(provider_id.to_ne_bytes()).rotate_left(32);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value.wrapping_mul(0x94d0_49bb_1331_11eb) ^ (value >> 31)
}

fn weighted_owner(entries: &[(i64, u32)], rotation: u64) -> i64 {
    let total = entries
        .iter()
        .map(|(_, weight)| u64::from(*weight))
        .sum::<u64>();
    let mut slot = rotation % total.max(1);
    for (id, weight) in entries {
        if slot < u64::from(*weight) {
            return *id;
        }
        slot -= u64::from(*weight);
    }
    entries[0].0
}
