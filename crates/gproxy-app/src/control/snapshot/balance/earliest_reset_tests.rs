use super::super::types::CredentialPressure;
use super::*;
use gproxy_core::{CredentialId, QuotaScope};
use rust_decimal::Decimal;

fn seed(member: i64, credential: i64) -> TargetSeed {
    let mut seed = super::tests::seed(member, 0, 100, credential);
    seed.credential_strategy = CredentialStrategy::EarliestReset;
    seed.upstream_model = "claude-opus".into();
    seed
}

fn window(used: i64, end: Option<i64>, scope: QuotaScope) -> CredentialPressure {
    CredentialPressure {
        cycle_id: 1,
        version: 1,
        last_observed_at: 100,
        used_percent: Decimal::from(used),
        period_end: end,
        scope,
    }
}

fn pressure(entries: &[(i64, i64, i64)]) -> CredentialPressureMap {
    entries
        .iter()
        .map(|&(id, used, end)| {
            (
                CredentialId(id),
                BTreeMap::from([("window".into(), window(used, Some(end), QuotaScope::All))]),
            )
        })
        .collect()
}

fn pick(
    seeds: Vec<TargetSeed>,
    pressure: &CredentialPressureMap,
    counters: &RotationCounters,
) -> Vec<TargetSeed> {
    super::order(
        seeds,
        RouteStrategy::Failover,
        1,
        None,
        &SelectionState {
            health: &BTreeMap::new(),
            pressure,
            now: 100,
        },
        counters,
    )
}

#[test]
fn spends_earliest_reset_even_above_ninety_percent() {
    let seeds = vec![seed(1, 11), seed(1, 22), seed(1, 33)];
    let pressure = pressure(&[(11, 20, 500), (22, 99, 200), (33, 0, 400)]);
    let counters = RotationCounters::default();
    for _ in 0..6 {
        let ordered = pick(seeds.clone(), &pressure, &counters);
        assert_eq!(
            ordered.iter().map(|s| s.credential.0).collect::<Vec<_>>(),
            [22, 33, 11]
        );
    }
}

#[test]
fn equal_reset_dates_rotate_by_weight_only_among_the_best_bucket() {
    let mut seeds = vec![seed(1, 11), seed(1, 22), seed(1, 33)];
    seeds[0].credential_weight = 300;
    seeds[2].credential_weight = 10_000;
    let pressure = pressure(&[(11, 95, 200), (22, 0, 200), (33, 0, 300)]);
    let counters = RotationCounters::default();
    let picks = (0..8)
        .map(|_| pick(seeds.clone(), &pressure, &counters)[0].credential.0)
        .collect::<Vec<_>>();
    assert_eq!(picks.iter().filter(|&&id| id == 11).count(), 6);
    assert_eq!(picks.iter().filter(|&&id| id == 22).count(), 2);
    assert!(!picks.contains(&33));
}

#[test]
fn reset_boundary_and_new_observations_change_the_winner_without_reload() {
    let seeds = vec![seed(1, 11), seed(1, 22)];
    let counters = RotationCounters::default();
    let mut pressure = pressure(&[(11, 0, 101), (22, 0, 200)]);
    assert_eq!(
        pick(seeds.clone(), &pressure, &counters)[0].credential.0,
        11
    );
    let ordered = super::order(
        seeds.clone(),
        RouteStrategy::Failover,
        1,
        None,
        &SelectionState {
            health: &BTreeMap::new(),
            pressure: &pressure,
            now: 101,
        },
        &counters,
    );
    assert_eq!(ordered[0].credential.0, 22);
    pressure
        .get_mut(&CredentialId(11))
        .unwrap()
        .get_mut("window")
        .unwrap()
        .period_end = Some(300);
    assert_eq!(pick(seeds, &pressure, &counters)[0].credential.0, 22);
}

#[test]
fn missing_dates_fall_back_to_weighted_rotation_and_exhaustion_is_last() {
    let mut seeds = vec![seed(1, 11), seed(1, 22), seed(1, 33)];
    seeds[0].credential_weight = 300;
    let pressure = pressure(&[(33, 100, 101)]);
    let counters = RotationCounters::default();
    let picks = (0..8)
        .map(|_| pick(seeds.clone(), &pressure, &counters)[0].credential.0)
        .collect::<Vec<_>>();
    assert_eq!(picks.iter().filter(|&&id| id == 11).count(), 6);
    assert_eq!(picks.iter().filter(|&&id| id == 22).count(), 2);
    assert!(!picks.contains(&33));
    assert_eq!(
        pick(seeds, &pressure, &counters)
            .last()
            .unwrap()
            .credential
            .0,
        33
    );
}

#[test]
fn earliest_reset_does_not_override_provider_failover_tiers_or_health() {
    let mut fallback = seed(3, 33);
    fallback.tier = 1;
    let seeds = vec![seed(1, 11), seed(2, 22), fallback];
    let pressure = pressure(&[(11, 20, 500), (22, 0, 200), (33, 0, 101)]);
    assert_eq!(
        pick(seeds.clone(), &pressure, &RotationCounters::default())[0]
            .credential
            .0,
        11
    );
    let health = BTreeMap::from([(
        CredentialId(11),
        BTreeMap::from([(
            "*".into(),
            (0, gproxy_store::records::CredentialHealthState::Dead),
        )]),
    )]);
    let ordered = super::order(
        seeds,
        RouteStrategy::Failover,
        1,
        None,
        &SelectionState {
            health: &health,
            pressure: &pressure,
            now: 100,
        },
        &RotationCounters::default(),
    );
    assert_eq!(
        ordered.iter().map(|s| s.credential.0).collect::<Vec<_>>(),
        [22, 33]
    );

    let health = BTreeMap::from([(
        CredentialId(22),
        BTreeMap::from([(
            "claude-opus".into(),
            (0, gproxy_store::records::CredentialHealthState::Degraded),
        )]),
    )]);
    let ordered = super::order(
        vec![seed(1, 11), seed(1, 22)],
        RouteStrategy::Failover,
        1,
        None,
        &SelectionState {
            health: &health,
            pressure: &pressure,
            now: 100,
        },
        &RotationCounters::default(),
    );
    assert_eq!(ordered[0].credential.0, 11);
}

#[test]
fn quota_ranking_respects_model_scope_both_windows_and_unknown_dates() {
    use super::super::pressure::earliest_reset;
    let mut windows = BTreeMap::from([
        (
            "3p-5h".into(),
            window(
                99,
                Some(120),
                QuotaScope::ModelPrefixes(vec!["claude".into()]),
            ),
        ),
        (
            "3p-weekly".into(),
            window(
                20,
                Some(500),
                QuotaScope::ModelPrefixes(vec!["claude".into()]),
            ),
        ),
        (
            "gemini-weekly".into(),
            window(
                100,
                Some(110),
                QuotaScope::ModelPrefixes(vec!["gemini".into()]),
            ),
        ),
    ]);
    assert_eq!(earliest_reset(Some(&windows), "claude-opus", 100), (0, 120));
    assert_eq!(earliest_reset(Some(&windows), "gemini-pro", 100).0, 2);
    assert_eq!(earliest_reset(Some(&windows), "claudex", 100).0, 1);
    windows.get_mut("3p-weekly").unwrap().used_percent = Decimal::ONE_HUNDRED;
    assert_eq!(earliest_reset(Some(&windows), "claude-opus", 100).0, 2);
    windows.get_mut("3p-weekly").unwrap().period_end = Some(100);
    assert_eq!(earliest_reset(Some(&windows), "claude-opus", 100), (0, 120));
    windows.get_mut("3p-5h").unwrap().scope = QuotaScope::Models(vec!["claude-opus".into()]);
    assert_eq!(earliest_reset(Some(&windows), "claude-sonnet", 100).0, 1);
    windows.get_mut("3p-5h").unwrap().period_end = None;
    assert_eq!(earliest_reset(Some(&windows), "claude-opus", 100).0, 1);
    windows.get_mut("3p-5h").unwrap().used_percent = Decimal::ONE_HUNDRED;
    assert_eq!(earliest_reset(Some(&windows), "claude-opus", 100).0, 2);
    windows.get_mut("3p-5h").unwrap().scope = QuotaScope::Unknown;
    assert_eq!(earliest_reset(Some(&windows), "claude-opus", 100).0, 1);
}
