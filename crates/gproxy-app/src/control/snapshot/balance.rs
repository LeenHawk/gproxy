use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Mutex;

use super::types::{CredentialHealthMap, TargetSeed};

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
    health: &CredentialHealthMap,
    counters: &RotationCounters,
) -> Vec<TargetSeed> {
    seeds.retain(|seed| {
        !health
            .get(&seed.credential)
            .is_some_and(|(version, dead)| *dead && *version == seed.credential_version)
    });
    seeds.sort_by_key(|seed| {
        (
            seed.tier,
            Reverse(seed.member_weight),
            seed.member_id,
            Reverse(seed.credential_weight),
            seed.credential.0,
        )
    });
    let Some(primary_tier) = seeds.first().map(|seed| seed.tier) else {
        return seeds;
    };
    let primary_end = seeds
        .iter()
        .position(|seed| seed.tier != primary_tier)
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
    let credential_id = weighted_owner(&credentials, counters.next((1, balance_key, member_id)));
    if let Some(index) = seeds
        .iter()
        .position(|seed| seed.member_id == member_id && seed.credential.0 == credential_id)
    {
        let selected = seeds.remove(index);
        seeds.insert(0, selected);
    }
    seeds
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
            proxy_url: None,
            fingerprint: None,
            upstream_model: "model".into(),
        }
    }

    #[test]
    fn weighted_rotation_is_reproducible_and_never_leads_with_a_later_tier() {
        let seeds = vec![seed(1, 0, 7, 11), seed(2, 0, 3, 22), seed(3, 1, 100, 33)];
        let picks = |counters: &RotationCounters| {
            (0..10)
                .map(|_| {
                    order(seeds.clone(), 9, &BTreeMap::new(), counters)[0]
                        .credential
                        .0
                })
                .collect::<Vec<_>>()
        };
        let expected = vec![11, 11, 11, 11, 11, 11, 11, 22, 22, 22];
        assert_eq!(picks(&RotationCounters::default()), expected);
        assert_eq!(picks(&RotationCounters::default()), expected);
    }

    #[test]
    fn unhealthy_members_are_removed_before_the_rotation_slot_is_consumed() {
        let seeds = vec![seed(1, 0, 1, 11), seed(2, 0, 1, 22), seed(3, 0, 1, 33)];
        let health = BTreeMap::from([(gproxy_channel_api::CredentialId(11), (0, true))]);
        let counters = RotationCounters::default();
        let picks = (0..4)
            .map(|_| order(seeds.clone(), 4, &health, &counters)[0].credential.0)
            .collect::<Vec<_>>();
        assert_eq!(picks, vec![22, 33, 22, 33]);
    }
}
