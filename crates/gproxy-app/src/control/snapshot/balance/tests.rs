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
    let seeds = vec![seed(1, 0, 1, 11), seed(1, 0, 1, 22), seed(3, 1, 100, 33)];
    let picks = |counters: &RotationCounters| {
        (0..4)
            .map(|_| {
                order(
                    seeds.clone(),
                    RouteStrategy::RoundRobin,
                    9,
                    None,
                    &BTreeMap::new(),
                    counters,
                )[0]
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
            order(
                sticky.clone(),
                RouteStrategy::RoundRobin,
                9,
                Some(41),
                &BTreeMap::new(),
                &counters,
            )[0]
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
    let ordered = order(
        seeds,
        RouteStrategy::RoundRobin,
        4,
        None,
        &health,
        &counters,
    );
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
    let ordered = order(
        vec![degraded, healthy],
        RouteStrategy::RoundRobin,
        4,
        None,
        &health,
        &counters,
    );
    assert_eq!(ordered[0].credential.0, 22);
    assert_eq!(ordered[1].credential.0, 11);
}

#[test]
fn route_strategies_preserve_rotation_ratios_and_fixed_failover_order() {
    let seeds = vec![
        seed(1, 0, 100, 11),
        seed(2, 0, 100, 22),
        seed(3, 1, 1000, 33),
    ];
    for strategy in [RouteStrategy::RoundRobin, RouteStrategy::Weighted] {
        let counters = RotationCounters::default();
        let picks = (0..8)
            .map(|_| {
                order(
                    seeds.clone(),
                    strategy,
                    7,
                    None,
                    &BTreeMap::new(),
                    &counters,
                )[0]
                .member_id
            })
            .collect::<Vec<_>>();
        assert_eq!(picks, [1, 2, 1, 2, 1, 2, 1, 2]);
    }
    let mut unequal = seeds;
    unequal[0].member_weight = 300;
    for (strategy, expected) in [
        (RouteStrategy::RoundRobin, 4),
        (RouteStrategy::Weighted, 6),
        (RouteStrategy::Failover, 8),
    ] {
        let counters = RotationCounters::default();
        let picks = (0..8)
            .map(|_| {
                order(
                    unequal.clone(),
                    strategy,
                    7,
                    None,
                    &BTreeMap::new(),
                    &counters,
                )[0]
                .member_id
            })
            .collect::<Vec<_>>();
        assert_eq!(picks.iter().filter(|id| **id == 1).count(), expected);
        assert!(!picks.contains(&3));
    }
}

#[test]
fn round_robin_rotates_complete_member_groups_and_failover_skips_dead_members() {
    let seeds = vec![
        seed(1, 0, 300, 11),
        seed(1, 0, 300, 12),
        seed(2, 0, 100, 22),
        seed(3, 0, 1, 33),
    ];
    let counters = RotationCounters::default();
    order(
        seeds.clone(),
        RouteStrategy::RoundRobin,
        8,
        None,
        &BTreeMap::new(),
        &counters,
    );
    let ordered = order(
        seeds.clone(),
        RouteStrategy::RoundRobin,
        8,
        None,
        &BTreeMap::new(),
        &counters,
    );
    assert_eq!(
        ordered
            .iter()
            .map(|seed| seed.member_id)
            .collect::<Vec<_>>(),
        [2, 3, 1, 1]
    );
    let health = [11, 12]
        .into_iter()
        .map(|id| {
            (
                gproxy_channel_api::CredentialId(id),
                BTreeMap::from([(
                    "*".into(),
                    (0, gproxy_store::records::CredentialHealthState::Dead),
                )]),
            )
        })
        .collect();
    let ordered = order(seeds, RouteStrategy::Failover, 8, None, &health, &counters);
    assert_eq!(ordered[0].member_id, 2);
    assert_eq!(ordered.len(), 2);
}

#[test]
fn weighted_rotation_resets_after_pool_members_or_weights_change() {
    let counters = RotationCounters::default();
    let mut seeds = vec![seed(1, 0, 100, 11), seed(2, 0, 100, 22)];
    assert_eq!(
        order(
            seeds.clone(),
            RouteStrategy::Weighted,
            7,
            None,
            &BTreeMap::new(),
            &counters
        )[0]
        .member_id,
        1
    );
    seeds[0].member_weight = 300;
    let picks = (0..8)
        .map(|_| {
            order(
                seeds.clone(),
                RouteStrategy::Weighted,
                7,
                None,
                &BTreeMap::new(),
                &counters,
            )[0]
            .member_id
        })
        .collect::<Vec<_>>();
    assert_eq!(picks.iter().filter(|id| **id == 1).count(), 6);
    seeds.remove(0);
    assert_eq!(
        order(
            seeds,
            RouteStrategy::Weighted,
            7,
            None,
            &BTreeMap::new(),
            &counters
        )[0]
        .member_id,
        2
    );
}
