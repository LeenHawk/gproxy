//! Pure member/credential ordering (§3.3). Sync and deterministic — no cache,
//! no awaits, no randomness (counter-based weighted rotation keeps wasm and
//! tests deterministic). Callers resolve provider config, rotation counters,
//! and sticky pins.

use std::cmp::Ordering;
use std::sync::Arc;

use crate::health::HealthState;
use crate::health::breaker::Admit;
use crate::health::config::BreakerConfig;
use crate::store::persistence::records::{Credential, RouteMember};

/// Healthy members in final attempt order. `cfg_for` returns the provider's
/// breaker config, or `None` when the provider is missing/disabled (member
/// excluded). Breaker-open members are excluded; the lowest surviving tier is
/// the primary group ordered per `strategy`; remaining healthy members (already
/// `(tier asc, weight desc)`) trail as the failover tail. `rotation` is only
/// consumed by strategies that rotate (`round_robin` / `weighted`).
pub fn order_members<'a>(
    strategy: &str,
    members: &'a [RouteMember],
    cfg_for: impl Fn(&RouteMember) -> Option<BreakerConfig>,
    health: &HealthState,
    rotation: impl FnOnce() -> usize,
    now: i64,
) -> Vec<&'a RouteMember> {
    let healthy: Vec<&RouteMember> = members
        .iter()
        .filter(|m| {
            cfg_for(m).is_some_and(|cfg| {
                !matches!(health.admit_member(m.id, &cfg, now), Admit::No { .. })
            })
        })
        .collect();
    let Some(first) = healthy.first() else {
        return healthy;
    };
    let tier = first.tier;
    let split = healthy
        .iter()
        .position(|m| m.tier != tier)
        .unwrap_or(healthy.len());
    let (primary, tail) = healthy.split_at(split);
    let mut out = order_primary(strategy, primary, health, rotation);
    out.extend_from_slice(tail);
    out
}

/// Order the (non-empty) primary group per strategy.
fn order_primary<'a>(
    strategy: &str,
    primary: &[&'a RouteMember],
    health: &HealthState,
    rotation: impl FnOnce() -> usize,
) -> Vec<&'a RouteMember> {
    match strategy {
        "failover" => primary.to_vec(),
        "round_robin" => rotate(primary, rotation()),
        "weighted" => weighted(primary, rotation()),
        "least_latency" => {
            let mut out = primary.to_vec();
            out.sort_by(|a, b| {
                cmp_latency(health.latency(a.id), health.latency(b.id))
                    .then(b.weight.cmp(&a.weight))
            });
            out
        }
        other => {
            tracing::warn!(
                strategy = other,
                "unknown route strategy; using failover order"
            );
            primary.to_vec()
        }
    }
}

fn rotate<'a>(primary: &[&'a RouteMember], rotation: usize) -> Vec<&'a RouteMember> {
    let start = rotation % primary.len();
    let mut out = Vec::with_capacity(primary.len());
    out.extend_from_slice(&primary[start..]);
    out.extend_from_slice(&primary[..start]);
    out
}

/// Deterministic weighted rotation: conceptually expand members by weight and
/// index the slot owner with `rotation % total_weight`; the owner goes first,
/// the rest keep weight-desc order. All-zero/negative weights fall back to the
/// failover order.
fn weighted<'a>(primary: &[&'a RouteMember], rotation: usize) -> Vec<&'a RouteMember> {
    let total: u64 = primary.iter().map(|m| m.weight.max(0) as u64).sum();
    if total == 0 {
        return primary.to_vec();
    }
    let mut slot = rotation as u64 % total;
    let mut pick = 0;
    for (i, m) in primary.iter().enumerate() {
        let w = m.weight.max(0) as u64;
        if slot < w {
            pick = i;
            break;
        }
        slot -= w;
    }
    let mut out = primary.to_vec();
    let chosen = out.remove(pick);
    out.insert(0, chosen);
    out
}

/// `None` (untried) sorts FIRST — optimistic.
fn cmp_latency(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
    }
}

/// Order an already health-filtered credential pool: rotate by `rotation`,
/// then a `pinned` credential present in the pool moves to the front (sticky).
pub fn order_credentials(
    creds: &[Arc<Credential>],
    rotation: usize,
    pinned: Option<i64>,
) -> Vec<Arc<Credential>> {
    if creds.is_empty() {
        return Vec::new();
    }
    let start = rotation % creds.len();
    let mut out: Vec<Arc<Credential>> = creds[start..]
        .iter()
        .chain(creds[..start].iter())
        .cloned()
        .collect();
    if let Some(pin) = pinned
        && let Some(pos) = out.iter().position(|c| c.id == pin)
        && pos > 0
    {
        let c = out.remove(pos);
        out.insert(0, c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: i64, tier: i64, weight: i64) -> RouteMember {
        RouteMember {
            id,
            route_id: 1,
            provider_id: 1,
            upstream_model_id: "m".into(),
            weight,
            tier,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn cfg(_: &RouteMember) -> Option<BreakerConfig> {
        Some(BreakerConfig::default())
    }

    fn ids(out: &[&RouteMember]) -> Vec<i64> {
        out.iter().map(|m| m.id).collect()
    }

    #[test]
    fn failover_skips_open_member_and_keeps_tail() {
        let health = HealthState::new();
        let bc = BreakerConfig::default();
        let members = vec![member(1, 0, 100), member(2, 0, 50), member(3, 1, 10)];
        for _ in 0..5 {
            health.record_member(1, &bc, false, 100); // open until 130
        }
        let out = order_members("failover", &members, cfg, &health, || 0, 100);
        assert_eq!(
            ids(&out),
            vec![2, 3],
            "open member excluded, tier-1 tail kept"
        );
    }

    #[test]
    fn round_robin_rotates_across_calls() {
        let health = HealthState::new();
        let members = vec![member(1, 0, 1), member(2, 0, 1)];
        let rot = || health.next_route_rotation(9);
        let first = order_members("round_robin", &members, cfg, &health, rot, 0);
        let second = order_members("round_robin", &members, cfg, &health, rot, 0);
        assert_eq!(ids(&first), vec![1, 2]);
        assert_eq!(ids(&second), vec![2, 1]);
    }

    #[test]
    fn weighted_slots_follow_weights() {
        let health = HealthState::new();
        let members = vec![member(1, 0, 3), member(2, 0, 1)];
        let picks: Vec<i64> = (0..4)
            .map(|_| {
                let rot = || health.next_route_rotation(5);
                order_members("weighted", &members, cfg, &health, rot, 0)[0].id
            })
            .collect();
        assert_eq!(picks, vec![1, 1, 1, 2], "weight 3:1 over 4 rotations");
    }

    #[test]
    fn least_latency_orders_untried_first_then_ascending() {
        let health = HealthState::new();
        health.record_latency(1, 200.0);
        health.record_latency(2, 100.0);
        let members = vec![member(1, 0, 3), member(2, 0, 2), member(3, 0, 1)];
        let out = order_members("least_latency", &members, cfg, &health, || 0, 0);
        assert_eq!(ids(&out), vec![3, 2, 1], "untried first, then EWMA asc");
    }
}
