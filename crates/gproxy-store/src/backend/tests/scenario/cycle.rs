use rust_decimal::Decimal;
mod ordering;
mod regression;
use serde_json::json;

use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaBoundaryConfidence,
    QuotaBoundarySource, QuotaCoverage, QuotaCycleCloseReason, QuotaCycleStatus,
};
use crate::{Store, StoreError};

#[derive(Debug, PartialEq)]
pub(super) struct Outcome {
    history: Vec<CredentialQuotaCycleRecord>,
    pressure: Decimal,
}

pub(super) async fn run(store: &Store, credential_id: i64) -> Result<Outcome, StoreError> {
    let first = store
        .observe_credential_quota_cycle(&observation(credential_id, "primary", 0, 100, 10, 10))
        .await?;
    assert_eq!(first.status, QuotaCycleStatus::Open);
    assert_eq!(first.coverage, QuotaCoverage::PartialLowerBound);
    let serialized = serde_json::to_value(&first).expect("serialize cycle");
    assert!(serialized.get("used_percent").is_none());

    let updated = store
        .observe_credential_quota_cycle(&observation(credential_id, "primary", 0, 100, 20, 25))
        .await?;
    assert_eq!(updated.id, first.id);
    assert_eq!(updated.last_observed_at, 20);

    let mut secondary = observation(credential_id, "secondary", 0, 300, 20, 1);
    secondary.used_percent = Some(Decimal::from(70));
    store.observe_credential_quota_cycle(&secondary).await?;
    let mut expired = observation(credential_id, "expired", 0, 25, 20, 99);
    expired.used_percent = Some(Decimal::from(99));
    store.observe_credential_quota_cycle(&expired).await?;
    let mut inferred = observation(credential_id, "inferred", 0, 25, 20, 1);
    inferred.boundary_source = QuotaBoundarySource::Inferred;
    inferred.boundary_confidence = QuotaBoundaryConfidence::Derived;
    inferred.used_percent = Some(Decimal::from(80));
    store.observe_credential_quota_cycle(&inferred).await?;

    let pressures = store.credential_quota_pressures(30).await?;
    assert_eq!(pressures.len(), 3);
    assert_eq!(
        store.credential_quota_pressure(credential_id, 30).await?,
        Some(Decimal::from(80))
    );

    let crossed = store
        .observe_credential_quota_cycle(&observation(credential_id, "primary", 0, 500, 110, 5))
        .await?;
    assert_ne!(crossed.id, first.id);
    assert_eq!(crossed.period_start, Some(0));
    assert_eq!(crossed.accounting_start_ms, 100_000);
    assert_eq!(crossed.coverage, QuotaCoverage::FullPeriodLowerBound);
    let history = store
        .credential_quota_cycle_history(credential_id, "primary")
        .await?;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].status, QuotaCycleStatus::Open);
    assert_eq!(history[1].status, QuotaCycleStatus::Closed);
    assert_eq!(
        history[1].close_reason,
        Some(QuotaCycleCloseReason::BoundaryCrossed)
    );
    assert_eq!(history[1].period_end, Some(100));
    assert_eq!(history[1].boundary_source, QuotaBoundarySource::Upstream);

    let closed = store
        .close_credential_quota_cycle(crossed.id, QuotaCycleCloseReason::ManualReset, 120)
        .await?
        .expect("closed cycle");
    assert_eq!(closed.status, QuotaCycleStatus::Closed);
    assert_eq!(
        closed.close_reason,
        Some(QuotaCycleCloseReason::ManualReset)
    );
    let mut stale_observation = observation(credential_id, "primary", 0, 120, 121, 99);
    stale_observation.period_start = None;
    stale_observation.boundary_source = QuotaBoundarySource::Inferred;
    let stale = store
        .observe_credential_quota_cycle(&stale_observation)
        .await?;
    assert_eq!(stale.id, closed.id);
    assert_eq!(stale.status, QuotaCycleStatus::Closed);

    let reopened = store
        .observe_credential_quota_cycle(&observation(credential_id, "primary", 120, 820, 121, 95))
        .await?;
    assert_ne!(reopened.id, crossed.id);
    assert_eq!(reopened.coverage, QuotaCoverage::PartialLowerBound);
    let mut same_observation = observation(credential_id, "primary", 120, 300, 121, 2);
    same_observation.boundary_source = QuotaBoundarySource::Inferred;
    same_observation.boundary_confidence = QuotaBoundaryConfidence::Derived;
    let same_second = store
        .observe_credential_quota_cycle(&same_observation)
        .await?;
    assert_eq!(same_second.id, reopened.id);
    assert!(same_second.version > reopened.version);
    assert_eq!(same_second.upstream_used, Some(Decimal::from(95)));
    assert_eq!(same_second.period_end, Some(820));
    assert_eq!(same_second.boundary_source, QuotaBoundarySource::Upstream);

    let local = store
        .observe_credential_quota_cycle(&observation(
            credential_id,
            "local",
            3_600,
            5_000,
            4_000,
            1,
        ))
        .await?;
    assert_eq!(local.metrics["requests"], json!("2"));
    assert_eq!(local.metrics["input_tokens"], json!("17"));
    assert_eq!(local.metrics["output_tokens"], json!("8"));
    assert_eq!(local.metrics["cached_input_tokens"], json!("3"));
    assert_eq!(local.metrics["audio_seconds"], json!("3"));
    assert_eq!(local.models.len(), 2);
    let model_requests = local
        .models
        .iter()
        .map(|model| {
            model.metrics["requests"]
                .as_str()
                .unwrap()
                .parse::<i64>()
                .unwrap()
        })
        .sum::<i64>();
    assert_eq!(json!(model_requests.to_string()), local.metrics["requests"]);

    let trusted = store
        .observe_credential_quota_cycle(&observation(credential_id, "trusted", 0, 200, 10, 95))
        .await?;
    let mut correction = observation(credential_id, "trusted", 95, 300, 110, 99);
    correction.boundary_source = QuotaBoundarySource::Inferred;
    correction.boundary_confidence = QuotaBoundaryConfidence::Derived;
    let held = store.observe_credential_quota_cycle(&correction).await?;
    assert_eq!(held.id, trusted.id);
    assert_eq!(held.period_end, Some(200));
    assert_eq!(held.upstream_used, Some(Decimal::from(99)));
    let trusted_history = store
        .credential_quota_cycle_history(credential_id, "trusted")
        .await?;
    assert_eq!(trusted_history.len(), 1);
    assert_eq!(
        trusted_history[0].boundary_source,
        QuotaBoundarySource::Upstream
    );

    store
        .observe_credential_quota_cycle(&observation(credential_id, "full", 0, 500, 10, 1))
        .await?;
    let full = store
        .observe_credential_quota_cycle(&observation(credential_id, "full", 500, 1000, 510, 1))
        .await?;
    assert_eq!(full.coverage, QuotaCoverage::FullPeriodLowerBound);
    let gap = store
        .observe_credential_quota_cycle(&observation(credential_id, "gap", 0, 100, 10, 1))
        .await?;
    store
        .close_credential_quota_cycle(gap.id, QuotaCycleCloseReason::BoundaryCrossed, 100)
        .await?;
    let resumed = store
        .observe_credential_quota_cycle(&observation(credential_id, "gap", 0, 500, 110, 1))
        .await?;
    assert_eq!(resumed.period_start, Some(0));
    assert_eq!(resumed.accounting_start_ms, 100_000);
    let skipped = store
        .observe_credential_quota_cycle(&observation(credential_id, "skip", 0, 100, 10, 1))
        .await?;
    store
        .close_credential_quota_cycle(skipped.id, QuotaCycleCloseReason::BoundaryCrossed, 100)
        .await?;
    let skipped = store
        .observe_credential_quota_cycle(&observation(credential_id, "skip", 500, 600, 510, 1))
        .await?;
    assert_eq!(skipped.period_start, Some(500));
    let history = store
        .credential_quota_cycle_history(credential_id, "primary")
        .await?;
    assert_eq!(history.len(), 3);

    Ok(Outcome {
        history,
        pressure: store
            .credential_quota_pressure(credential_id, 121)
            .await?
            .expect("credential pressure"),
    })
}

fn observation(
    credential_id: i64,
    window_key: &str,
    period_start: i64,
    period_end: i64,
    observed_at: i64,
    used: i64,
) -> CredentialQuotaObservation {
    CredentialQuotaObservation {
        unit: Some("requests".into()),
        reset_behavior: gproxy_core::QuotaResetBehavior::Periodic,
        scope: gproxy_core::QuotaScope::All,
        sample: gproxy_core::QuotaSample {
            source: gproxy_core::QuotaSampleSource::Unknown,
            started_at_ms: observed_at * 1000,
            received_at_ms: observed_at * 1000,
        },
        credential_id,
        window_key: window_key.into(),
        label: None,
        period_start: Some(period_start),
        period_end: Some(period_end),
        boundary_source: QuotaBoundarySource::Upstream,
        boundary_confidence: QuotaBoundaryConfidence::Exact,
        observed_at,
        upstream_used: Some(Decimal::from(used)),
        upstream_limit: Some(Decimal::from(100)),
        used_percent: None,
    }
}
