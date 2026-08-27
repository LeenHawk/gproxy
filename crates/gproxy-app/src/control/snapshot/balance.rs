use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Mutex;

use super::types::{CredentialHealthMap, CredentialStrategy, TargetSeed};

#[derive(Default)]
pub(super) struct RotationCounters(Mutex<BTreeMap<(u8, i64, i64), u64>>);

impl RotationCounters {
    fn next(&self, key: (u8, i64, i64)) -> u64 {
        let mut counters = self.0.lock().expect("rotation counter lock");
        let value = counters.entry(key).or_default();
        let current = *value;
        *value = value.wrapping_add(1);
        current
    }
}

pub(super) fn order(
    mut seeds: Vec<TargetSeed>,
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
    let member_id = weighted_owner(&members, counters.next((0, balance_key, 0)));
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
    let rotation = match strategy {
        CredentialStrategy::RoundRobin => counters.next((1, balance_key, member_id)),
        CredentialStrategy::Sticky => affinity.map_or(0, |key| stable_slot(key, member_id)),
    };
    let credential_id = weighted_owner(&credentials, rotation);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(member_id: i64, tier: u32, member_weight: u32, credential: i64) -> TargetSeed {
        TargetSeed {
            member_id,
            tier,
            member_weight,
            provider_id: member_id,
            credential: gproxy_channel_api::CredentialId(credential),
            credential_version: 0,
            credential_weight: 100,
            credential_strategy: CredentialStrategy::RoundRobin,
            proxy_url: None,
            fingerprint: None,
            upstream_model: "model".into(),
        }
    }

    #[test]
    fn round_robin_rotates_while_sticky_keeps_one_weighted_owner() {
        let mut seeds = vec![seed(1, 0, 1, 11), seed(1, 0, 1, 22), seed(3, 1, 100, 33)];
        for seed in &mut seeds[..2] {
            seed.credential_weight = 1;
        }
        let picks = |counters: &RotationCounters| {
            (0..4)
                .map(|_| {
                    order(seeds.clone(), 9, None, &BTreeMap::new(), counters)[0]
                        .credential
                        .0
                })
                .collect::<Vec<_>>()
        };
        let expected = vec![11, 22, 11, 22];
        assert_eq!(picks(&RotationCounters::default()), expected);
        assert_eq!(picks(&RotationCounters::default()), expected);

        let mut sticky = seeds;
        for seed in &mut sticky {
            seed.credential_strategy = CredentialStrategy::Sticky;
        }
        let counters = RotationCounters::default();
        let sticky = (0..4)
            .map(|_| {
                order(sticky.clone(), 9, Some(41), &BTreeMap::new(), &counters)[0]
                    .credential
                    .0
            })
            .collect::<Vec<_>>();
        assert!(sticky.iter().all(|credential| *credential == sticky[0]));
    }

    #[test]
    fn unhealthy_members_are_removed_before_the_rotation_slot_is_consumed() {
        let mut blocked = seed(1, 0, 1, 11);
        blocked.upstream_model = "model-a".into();
        let mut isolated = seed(2, 0, 1, 11);
        isolated.upstream_model = "model-b".into();
        let seeds = vec![blocked, isolated, seed(3, 0, 1, 22)];
        let health = BTreeMap::from([(
            gproxy_channel_api::CredentialId(11),
            BTreeMap::from([(
                "model-a".into(),
                (0, gproxy_store::records::CredentialHealthState::Dead),
            )]),
        )]);
        let counters = RotationCounters::default();
        let ordered = order(seeds, 4, None, &health, &counters);
        assert!(
            !ordered
                .iter()
                .any(|seed| { seed.credential.0 == 11 && seed.upstream_model == "model-a" })
        );
        assert!(
            ordered
                .iter()
                .any(|seed| { seed.credential.0 == 11 && seed.upstream_model == "model-b" })
        );

        let mut degraded = seed(1, 0, 1, 11);
        degraded.upstream_model = "model-a".into();
        let healthy = seed(2, 0, 1, 22);
        let health = BTreeMap::from([(
            gproxy_channel_api::CredentialId(11),
            BTreeMap::from([(
                "model-a".into(),
                (0, gproxy_store::records::CredentialHealthState::Degraded),
            )]),
        )]);
        let ordered = order(vec![degraded, healthy], 4, None, &health, &counters);
        assert_eq!(ordered[0].credential.0, 22);
        assert_eq!(ordered[1].credential.0, 11);
    }
}
