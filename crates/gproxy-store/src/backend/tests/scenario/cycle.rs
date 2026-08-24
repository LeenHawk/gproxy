use rust_decimal::Decimal;
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
    assert!(
        serde_json::to_value(&first)
            .expect("serialize cycle")
            .get("used_percent")
            .is_none()
    );

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

    let pressures = store.credential_quota_pressures(30).await?;
    assert_eq!(pressures.len(), 2);
    assert_eq!(
        store.credential_quota_pressure(credential_id, 30).await?,
        Some(Decimal::from(70))
    );

    let crossed = store
        .observe_credential_quota_cycle(&observation(credential_id, "primary", 100, 200, 110, 5))
        .await?;
    assert_ne!(crossed.id, first.id);
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

    let closed = store
        .close_credential_quota_cycle(crossed.id, QuotaCycleCloseReason::ManualReset, 120)
        .await?
        .expect("closed cycle");
    assert_eq!(closed.status, QuotaCycleStatus::Closed);
    assert_eq!(
        closed.close_reason,
        Some(QuotaCycleCloseReason::ManualReset)
    );
    assert!(
        store
            .open_credential_quota_cycles(credential_id, 120)
            .await?
            .iter()
            .all(|cycle| cycle.window_key != "primary")
    );

    let reopened = store
        .observe_credential_quota_cycle(&observation(credential_id, "primary", 120, 220, 121, 1))
        .await?;
    assert_ne!(reopened.id, crossed.id);
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
        credential_id,
        window_key: window_key.into(),
        period_start: Some(period_start),
        period_end: Some(period_end),
        boundary_source: QuotaBoundarySource::Upstream,
        boundary_confidence: QuotaBoundaryConfidence::Exact,
        observed_at,
        upstream_used: Some(Decimal::from(used)),
        upstream_limit: Some(Decimal::from(100)),
        used_percent: None,
        coverage: QuotaCoverage::PartialLowerBound,
        metrics: json!({"requests": used}),
    }
}
