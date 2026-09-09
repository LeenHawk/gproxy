use std::collections::BTreeMap;
use std::sync::Mutex;

type PoolKey = (u8, i64, i64);

#[derive(Default)]
pub(in crate::control::snapshot) struct RotationCounters {
    rotations: Mutex<BTreeMap<PoolKey, u64>>,
    weighted: Mutex<BTreeMap<PoolKey, WeightedPool>>,
}

impl RotationCounters {
    pub(super) fn next(&self, key: PoolKey) -> u64 {
        let mut counters = self.rotations.lock().expect("rotation counter lock");
        let value = counters.entry(key).or_default();
        let current = *value;
        *value = value.wrapping_add(1);
        current
    }

    pub(super) fn smooth(&self, key: PoolKey, entries: &[(i64, u32)]) -> i64 {
        let mut pools = self.weighted.lock().expect("weighted rotation lock");
        let pool = pools.entry(key).or_default();
        if pool.entries != entries {
            pool.entries = entries.to_vec();
            pool.current = vec![0; entries.len()];
        }
        let mut total = 0;
        let mut selected = 0;
        for (index, (_, weight)) in entries.iter().enumerate() {
            let weight = i64::from(*weight);
            total += weight;
            pool.current[index] += weight;
            if pool.current[index] > pool.current[selected] {
                selected = index;
            }
        }
        pool.current[selected] -= total;
        entries[selected].0
    }
}

#[derive(Default)]
struct WeightedPool {
    entries: Vec<(i64, u32)>,
    current: Vec<i64>,
}
