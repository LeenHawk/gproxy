use crate::Store;
use crate::backend::tests::{libsql_store, native_store};
use crate::records::{CredentialQuotaObservation, CycleObservationRecord};
use gproxy_core::QuotaSampleSource;
use rust_decimal::Decimal;

#[tokio::test]
async fn boundary_threshold_and_concurrent_observations_have_backend_parity() {
    let directory = tempfile::tempdir().unwrap();
    let (native, _) = native_store(directory.path().join("native.db"))
        .await
        .unwrap();
    let (remote, _) = libsql_store(directory.path().join("remote.db"))
        .await
        .unwrap();
    for store in [&native, &remote] {
        boundaries(store).await;
        ordering(store).await;
    }
    assert_eq!(
        native
            .credential_quota_cycle_history(7, "ordered")
            .await
            .unwrap(),
        remote
            .credential_quota_cycle_history(7, "ordered")
            .await
            .unwrap(),
    );
}

async fn boundaries(store: &Store) {
    for drift in [-301, -300, -8, 0, 8, 300, 301] {
        let window = format!("drift-{drift}");
        let mut initial = super::observation(7, &window, 1_000, 11_000, 1_000, 0);
        // Codex's rounded 0% is compatible with an already running cycle.
        initial.unit = None;
        initial.upstream_used = None;
        initial.upstream_limit = None;
        initial.used_percent = Some(Decimal::ZERO);
        initial.sample.source = QuotaSampleSource::Probe;
        let first = store
            .observe_credential_quota_cycle(&initial)
            .await
            .unwrap();
        assert_eq!(first.period_start, Some(1_000));
        let mut shifted = initial.clone();
        shifted.period_start = Some(1_000 + drift);
        shifted.period_end = Some(11_000 + drift);
        shifted.observed_at = 2_010;
        shifted.sample.started_at_ms = 2_010_000;
        shifted.sample.received_at_ms = 2_010_000;
        let next = store
            .observe_credential_quota_cycle(&shifted)
            .await
            .unwrap();
        assert_eq!(next.id != first.id, drift.abs() > 300, "drift {drift}");
        if drift.abs() <= 300 {
            assert_eq!(next.period_start, first.period_start);
            assert_eq!(next.period_end, first.period_end);
            assert_eq!(next.accounting_start_ms, first.accounting_start_ms);
            assert_eq!(next.tracking.baseline_at_ms, first.tracking.baseline_at_ms);
        }
        let samples = store
            .credential_quota_observations(&next, false)
            .await
            .unwrap();
        assert_eq!(samples.last().unwrap().raw.as_ref(), Some(&shifted));
    }
}

async fn ordering(store: &Store) {
    let first = store
        .observe_credential_quota_cycle(&reading(10_000, 20_000, 10))
        .await
        .unwrap();
    let later = store
        .observe_credential_quota_cycle(&reading(9_000, 21_000, 11))
        .await
        .unwrap();
    assert_eq!(later.id, first.id);
    assert!(!later.tracking.uncertain);
    assert_eq!(later.tracking.baseline_at_ms, first.tracking.baseline_at_ms);

    let conflict = reading(20_500, 22_000, 8);
    let pending = store
        .observe_credential_quota_cycle(&conflict)
        .await
        .unwrap();
    assert!(pending.tracking.uncertain);
    assert_eq!(
        pending.tracking.pending_observation.as_ref(),
        Some(&conflict)
    );
    let verified = store
        .observe_credential_quota_cycle(&reading(23_000, 24_000, 12))
        .await
        .unwrap();
    assert_eq!(verified.id, first.id);
    assert!(!verified.tracking.uncertain);
    assert!(verified.tracking.pending_observation.is_none());
    assert_eq!(
        verified.tracking.baseline_at_ms,
        first.tracking.baseline_at_ms
    );
    assert_eq!(
        verified.tracking.baseline_percent,
        first.tracking.baseline_percent
    );

    let stale = reading(18_000, 19_000, 1);
    let held = store.observe_credential_quota_cycle(&stale).await.unwrap();
    assert_eq!(held.upstream_used, verified.upstream_used);
    assert!(!held.tracking.uncertain);
    let rows = store
        .backend()
        .execute(crate::query::runtime::cycle_observations(&held).unwrap())
        .await
        .unwrap()
        .rows;
    let raw = rows
        .into_iter()
        .map(|row| {
            serde_json::from_str::<CycleObservationRecord>(row.text("snapshot_json").unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(raw.len(), 5);
    assert_eq!(raw.iter().filter(|sample| sample.rejected).count(), 2);
    assert!(
        raw.iter()
            .any(|sample| sample.raw.as_ref() == Some(&conflict))
    );
    assert!(raw.iter().any(|sample| sample.raw.as_ref() == Some(&stale)));
    assert_eq!(
        store
            .credential_quota_observations(&held, false)
            .await
            .unwrap()
            .len(),
        3
    );

    let pending = store
        .observe_credential_quota_cycle(&reading(23_500, 25_000, 2))
        .await
        .unwrap();
    assert_eq!(pending.id, first.id);
    let confirmed = store
        .observe_credential_quota_cycle(&reading(26_000, 27_000, 3))
        .await
        .unwrap();
    assert_ne!(confirmed.id, first.id);
    assert_eq!(confirmed.accounting_start_ms, 25_000);
}

fn reading(start: i64, end: i64, used: i64) -> CredentialQuotaObservation {
    let mut observation = super::observation(7, "ordered", 0, 10_000, end / 1000, used);
    observation.sample.started_at_ms = start;
    observation.sample.received_at_ms = end;
    observation.sample.source = QuotaSampleSource::Response;
    observation
}
