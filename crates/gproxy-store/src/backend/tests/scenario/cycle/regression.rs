use crate::Store;
use crate::backend::tests::{libsql_store, native_store};
use crate::records::{CredentialQuotaObservation, QuotaCycleCloseReason, UsageFilter, UsageInput};
use gproxy_core::{QuotaResetBehavior, QuotaSample, QuotaScope};
use rust_decimal::Decimal;
use serde_json::json;

#[tokio::test]
async fn quota_rounds_and_usage_records_have_backend_parity() {
    let directory = tempfile::tempdir().unwrap();
    let native_path = directory.path().join("native.db");
    let remote_path = directory.path().join("remote.db");
    let (native, _) = native_store(native_path.clone()).await.unwrap();
    let (remote, _) = libsql_store(remote_path.clone()).await.unwrap();
    for store in [&native, &remote] {
        exercise(store).await;
    }
    let (reopened, _) = native_store(native_path).await.unwrap();
    let native_history = reopened
        .credential_quota_cycle_history(7, "primary")
        .await
        .unwrap();
    let remote_history = remote
        .credential_quota_cycle_history(7, "primary")
        .await
        .unwrap();
    assert_eq!(native_history, remote_history);
    for (native_cycle, remote_cycle) in native_history.iter().zip(&remote_history) {
        assert_eq!(
            reopened
                .credential_quota_observations(native_cycle, true)
                .await
                .unwrap(),
            remote
                .credential_quota_observations(remote_cycle, true)
                .await
                .unwrap(),
        );
    }
    reopened.repair_credential_quota(7, 150).await.unwrap();
    assert_eq!(
        reopened
            .credential_quota_cycle_history(7, "primary")
            .await
            .unwrap(),
        native_history
    );
}

async fn exercise(store: &Store) {
    let first = store
        .observe_credential_quota_cycle(&reading(10_000, 10))
        .await
        .unwrap();
    store
        .begin_credential_usage("sample", 7, "model-a", 10_500)
        .await
        .unwrap();
    store
        .observe_credential_quota_cycle(&reading(20_000, 20))
        .await
        .unwrap();
    let pending = store
        .credential_quota_cycle_history(7, "primary")
        .await
        .unwrap();
    assert_eq!(
        pending[0].estimate.as_ref().unwrap().reason.as_deref(),
        Some("incomplete_usage")
    );
    let samples = store
        .credential_quota_observations(&pending[0], true)
        .await
        .unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(
        samples[1].estimate.as_ref().unwrap().reason.as_deref(),
        Some("incomplete_usage")
    );
    let sample = usage("sample", "model-a", 10_500, 21);
    assert!(store.record_usage(&sample).await.unwrap());
    assert!(!store.record_usage(&sample).await.unwrap());
    let estimated = store
        .credential_quota_cycle_history(7, "primary")
        .await
        .unwrap();
    assert_eq!(estimated[0].id, first.id);
    assert_eq!(estimated[0].metrics["requests"], json!("1"));
    assert_eq!(estimated[0].metrics["total_tokens"], json!("130"));
    assert_eq!(
        estimated[0].estimate.as_ref().unwrap().tokens,
        Some(Decimal::from(1300))
    );
    assert_eq!(
        estimated[0].estimate.as_ref().unwrap().cost,
        Some(Decimal::from(20))
    );

    let reset = store
        .observe_credential_quota_cycle(&reading(30_100, 5))
        .await
        .unwrap();
    let samples = store
        .credential_quota_observations(&estimated[0], true)
        .await
        .unwrap();
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.observed_at_ms)
            .collect::<Vec<_>>(),
        vec![10_000, 20_000]
    );
    assert_eq!(samples[0].estimate.as_ref().unwrap().tokens, None);
    assert_eq!(
        samples[1].estimate.as_ref().unwrap().tokens,
        Some(Decimal::from(1300))
    );
    assert!(
        store
            .credential_quota_observations(&estimated[0], false)
            .await
            .unwrap()
            .iter()
            .all(|sample| sample.estimate.is_none())
    );
    assert_ne!(reset.id, first.id);
    assert_eq!(reset.accounting_start_ms, 30_100);
    store
        .record_usage(&usage("long", "model-a", 29_000, 35))
        .await
        .unwrap();
    let history = store
        .credential_quota_cycle_history(7, "primary")
        .await
        .unwrap();
    assert_eq!(history[1].period_end, Some(100));
    assert_eq!(history[1].accounting_end_ms, Some(30_100));
    assert_eq!(
        history[1].close_reason,
        Some(QuotaCycleCloseReason::UsageDecreased)
    );
    assert_eq!(history[1].metrics["requests"], json!("2"));
    assert_eq!(history[0].metrics["requests"], json!("0"));

    let again = store
        .observe_credential_quota_cycle(&reading(30_200, 3))
        .await
        .unwrap();
    assert_ne!(again.id, reset.id);
    let stale = store
        .observe_credential_quota_cycle(&reading(29_900, 1))
        .await
        .unwrap();
    assert_eq!(stale.id, again.id);
    let mut overlap = reading(30_250, 1);
    overlap.sample.started_at_ms = 30_150;
    let held = store
        .observe_credential_quota_cycle(&overlap)
        .await
        .unwrap();
    assert_eq!(held.id, again.id);
    assert!(held.tracking.uncertain);
    let same = store
        .observe_credential_quota_cycle(&reading(30_200, 3))
        .await
        .unwrap();
    assert_eq!(same.id, again.id);
    assert_eq!(
        store
            .credential_quota_observations(&same, false)
            .await
            .unwrap()
            .len(),
        1
    );
    let verified = store
        .observe_credential_quota_cycle(&reading(30_300, 1))
        .await
        .unwrap();
    assert_ne!(verified.id, again.id);

    let mut expanded = reading(40_000, 1);
    expanded.upstream_limit = Some(Decimal::from(200));
    let expanded = store
        .observe_credential_quota_cycle(&expanded)
        .await
        .unwrap();
    assert_eq!(expanded.id, verified.id);
    assert_eq!(expanded.tracking.baseline_at_ms, 40_000);
    let mut scoped = reading(50_000, 4);
    scoped.scope = QuotaScope::Models(vec!["model-a".into()]);
    let scoped = store.observe_credential_quota_cycle(&scoped).await.unwrap();
    assert_eq!(scoped.id, verified.id);
    let samples = store
        .credential_quota_observations(&scoped, false)
        .await
        .unwrap();
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].upstream_limit, Some(Decimal::from(100)));
    assert_eq!(samples[1].upstream_limit, Some(Decimal::from(200)));
    assert_eq!(samples[0].scope, QuotaScope::All);
    assert_eq!(samples[2].scope, QuotaScope::Models(vec!["model-a".into()]));
    store
        .record_usage(&usage("outside", "model-b", 51_000, 52))
        .await
        .unwrap();
    store
        .record_usage(&usage("inside", "model-a", 51_001, 52))
        .await
        .unwrap();
    let scope = store
        .credential_quota_cycle_history(7, "primary")
        .await
        .unwrap();
    assert_eq!(scope[0].metrics["requests"], json!("1"));
    assert_eq!(scope[0].models.len(), 1);

    let mut next = reading(110_000, 0);
    next.period_start = Some(100);
    next.period_end = Some(200);
    let (left, right) = tokio::join!(
        store.observe_credential_quota_cycle(&next),
        store.observe_credential_quota_cycle(&next)
    );
    assert_eq!(left.unwrap().id, right.unwrap().id);
    let history = store
        .credential_quota_cycle_history(7, "primary")
        .await
        .unwrap();
    assert_eq!(history.len(), 5);
    assert_eq!(history[0].accounting_start_ms, 100_000);
    assert_eq!(history[1].period_end, Some(100));
    let (first_write, duplicate_write) =
        tokio::join!(store.record_usage(&sample), store.record_usage(&sample));
    assert!(!first_write.unwrap() && !duplicate_write.unwrap());

    let mut recovering = reading(10_000, 70);
    recovering.window_key = "recovering".into();
    recovering.reset_behavior = QuotaResetBehavior::Recovering;
    let old = store
        .observe_credential_quota_cycle(&recovering)
        .await
        .unwrap();
    recovering.upstream_used = Some(Decimal::from(30));
    recovering.sample = QuotaSample {
        started_at_ms: 20_000,
        received_at_ms: 20_000,
    };
    recovering.observed_at = 20;
    let recovered = store
        .observe_credential_quota_cycle(&recovering)
        .await
        .unwrap();
    assert_eq!(recovered.id, old.id);
    records(store).await;
}

/// Reset stamps wobble by a second between observations, and an unused rolling
/// window reports `reset_at = now + window` so its start walks forward on every
/// probe. Both read as boundary crossings and minted a fresh cycle each time,
/// restarting accounting and crowding quieter windows out of the console page;
/// a stamp landing a second ahead of our clock was rejected outright.
#[tokio::test]
async fn wobbling_boundaries_keep_one_cycle() {
    let directory = tempfile::tempdir().unwrap();
    let (store, _) = native_store(directory.path().join("wobble.db"))
        .await
        .unwrap();

    for (index, end) in [18_000, 17_999, 18_000, 18_001].into_iter().enumerate() {
        let at = 1_000 + index as i64;
        store
            .observe_credential_quota_cycle(&super::observation(
                7,
                "wobble",
                end - 18_000,
                end,
                at,
                40,
            ))
            .await
            .unwrap();
    }
    let wobble = store
        .credential_quota_cycle_history(7, "wobble")
        .await
        .unwrap();
    assert_eq!(wobble.len(), 1);
    assert_eq!(wobble[0].period_end, Some(18_000));

    for at in [2_000, 2_010, 2_020] {
        store
            .observe_credential_quota_cycle(&super::observation(
                7,
                "unused",
                at,
                at + 18_000,
                at,
                0,
            ))
            .await
            .unwrap();
    }
    let unused = store
        .credential_quota_cycle_history(7, "unused")
        .await
        .unwrap();
    assert_eq!(unused.len(), 1);
    assert_eq!(unused[0].period_start, None);

    // Usage proves the period began, so the same cycle adopts its boundary.
    let started = store
        .observe_credential_quota_cycle(&super::observation(
            7,
            "unused",
            2_030,
            2_030 + 18_000,
            2_030,
            12,
        ))
        .await
        .unwrap();
    assert_eq!(started.id, unused[0].id);
    assert_eq!(started.period_end, Some(2_030 + 18_000));

    store
        .observe_credential_quota_cycle(&super::observation(
            7,
            "ahead",
            3_001,
            3_001 + 18_000,
            3_000,
            0,
        ))
        .await
        .unwrap();
}

async fn records(store: &Store) {
    for index in 0..21 {
        store
            .record_usage(&usage(
                &format!("page-{index}"),
                "paged",
                120_000 + index,
                121,
            ))
            .await
            .unwrap();
    }
    let filter = UsageFilter {
        from: 120,
        to: 122,
        model: Some("paged".into()),
        ..Default::default()
    };
    let (first, total) = store.usage_records(&filter, 1, 10).await.unwrap();
    let (second, _) = store.usage_records(&filter, 2, 10).await.unwrap();
    let (last, _) = store.usage_records(&filter, 3, 10).await.unwrap();
    assert_eq!(
        (first.len(), second.len(), last.len(), total),
        (10, 10, 1, 21)
    );
    assert!(first[9].id > second[0].id && second[9].id > last[0].id);
    let summary = store.usage_summary(&filter).await.unwrap();
    assert_eq!(summary.requests, 21);
    assert_eq!(summary.cost, Decimal::from(42));
    assert_eq!(summary.total_tokens(), Decimal::from(2730));
    let exact = UsageFilter {
        request_id: Some("page-4".into()),
        user_key_id: Some(9),
        credential_id: Some(7),
        provider_id: Some(3),
        user_id: Some(8),
        operation: Some("generate".into()),
        ended: Some("complete".into()),
        usage_source: Some("upstream".into()),
        ..filter
    };
    assert_eq!(store.usage_records(&exact, 1, 10).await.unwrap().1, 1);
    assert_eq!(
        store.usage_summary(&exact).await.unwrap().cost,
        Decimal::from(2)
    );
}

fn reading(at_ms: i64, used: i64) -> CredentialQuotaObservation {
    let mut value = super::observation(7, "primary", 0, 100, at_ms / 1000, used);
    value.sample = QuotaSample {
        started_at_ms: at_ms,
        received_at_ms: at_ms,
    };
    value
}

fn usage(request: &str, model: &str, started: i64, at: i64) -> UsageInput {
    UsageInput {
        request_id: request.into(),
        at,
        upstream_started_at_ms: Some(started),
        provider_id: 3,
        credential_id: 7,
        organization_id: None,
        team_id: None,
        user_id: Some(8),
        user_key_id: Some(9),
        operation: Some("generate".into()),
        upstream_model: model.into(),
        input_tokens: 100,
        output_tokens: 20,
        cached_input_tokens: 80,
        metrics: json!({"cache_creation_5m_tokens": "10"}),
        dimensions: json!({}),
        cost: Decimal::from(2),
        usage_source: "upstream".into(),
        ended: "complete".into(),
        latency_ms: 1,
    }
}
