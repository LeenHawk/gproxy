use bytes::Bytes;
use gproxy_core::{ControlPlane, CoreError, Host, UsageSink};
use gproxy_protocol::SettleMode;
use gproxy_store::records::{QuotaInput, QuotaWindowKind};
use rust_decimal::Decimal;

use super::setup;
use crate::ControlMutation;

fn budget(credential: i64) -> QuotaInput {
    QuotaInput {
        subject_kind: "credential".into(),
        subject_id: credential,
        quota_total: None,
        quota_monthly: None,
        quota_weekly: None,
        quota_daily: None,
        quota_5h: None,
        quota_7d: None,
        enabled: true,
    }
}

#[tokio::test]
async fn credential_budget_settles_without_usage_logs_and_blocks_each_limit() {
    let fixture = setup::fixture().await;
    super::setting(&fixture.app, "enable_usage", serde_json::json!(false)).await;
    let mut input = budget(fixture.credential);
    input.quota_total = Some(Decimal::ONE);
    input.quota_monthly = Some(Decimal::ONE);
    input.quota_weekly = Some(Decimal::ONE);
    input.quota_daily = Some(Decimal::ONE);
    let id = setup::id(
        fixture
            .app
            .mutate(ControlMutation::Quota(input.clone()))
            .await
            .unwrap(),
    );
    let host = &fixture.app.inner.host;
    let plan = host
        .services
        .control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
        .unwrap();
    let target = &plan.targets[0];
    host.admit_credential(target, &Bytes::new(), SettleMode::OnResponse)
        .await
        .unwrap();
    let settlement = gproxy_core::Settlement {
        upstream_started_at_ms: None,
        request_id: "credential-budget-spend".into(),
        provider_id: fixture.provider,
        credential_id: target.credential,
        upstream_model: target.upstream_model.clone(),
        usage: Default::default(),
        cost: Decimal::ONE,
        source: gproxy_core::UsageSource::Upstream,
        ended: gproxy_core::Ended::Interrupted,
        latency_ms: 1,
    };
    tokio::join!(host.record(&settlement), host.record(&settlement));
    let windows: Vec<_> = host
        .services
        .store
        .quota_windows()
        .await
        .unwrap()
        .into_iter()
        .filter(|window| window.quota_id == id)
        .collect();
    assert_eq!(windows.len(), 4);
    assert!(
        windows
            .iter()
            .all(|window| window.cost_used == Decimal::ONE)
    );
    for kind in [
        QuotaWindowKind::Total,
        QuotaWindowKind::Monthly,
        QuotaWindowKind::Weekly,
        QuotaWindowKind::Daily,
    ] {
        let mut input = budget(fixture.credential);
        match kind {
            QuotaWindowKind::Total => input.quota_total = Some(Decimal::ONE),
            QuotaWindowKind::Monthly => input.quota_monthly = Some(Decimal::ONE),
            QuotaWindowKind::Weekly => input.quota_weekly = Some(Decimal::ONE),
            QuotaWindowKind::Daily => input.quota_daily = Some(Decimal::ONE),
            QuotaWindowKind::FiveHour | QuotaWindowKind::SevenDay => {
                panic!("not a calendar budget")
            }
        }
        host.services.store.update_quota(id, &input).await.unwrap();
        fixture.app.reload().await.unwrap();
        assert!(
            matches!(
                host.admit_credential(target, &Bytes::new(), SettleMode::OnResponse)
                    .await,
                Err(CoreError::QuotaExceeded)
            ),
            "{kind:?}"
        );
        host.admit_credential(target, &Bytes::new(), SettleMode::Free)
            .await
            .unwrap();
        let mut other = target.clone();
        other.credential = gproxy_core::CredentialId(fixture.credential + 100);
        host.admit_credential(&other, &Bytes::new(), SettleMode::OnResponse)
            .await
            .unwrap();
        input.enabled = false;
        host.services.store.update_quota(id, &input).await.unwrap();
        fixture.app.reload().await.unwrap();
        host.admit_credential(target, &Bytes::new(), SettleMode::OnResponse)
            .await
            .unwrap();
    }
    // Lifetime spend survives periodic rollovers and control-plane reloads.
    for window in windows {
        let next = host
            .services
            .store
            .ensure_quota_window(
                id,
                window.window_kind,
                window.reset_at.unwrap_or(2_000_000_000),
            )
            .await
            .unwrap();
        assert_eq!(
            next.cost_used,
            if window.window_kind == QuotaWindowKind::Total {
                Decimal::ONE
            } else {
                Decimal::ZERO
            }
        );
    }
}

#[tokio::test]
async fn credential_budget_zero_and_missing_prices_cannot_send_paid_requests() {
    let fixture = setup::fixture().await;
    let mut input = budget(fixture.credential);
    input.quota_daily = Some(Decimal::ZERO);
    let id = setup::id(
        fixture
            .app
            .mutate(ControlMutation::Quota(input.clone()))
            .await
            .unwrap(),
    );
    let host = &fixture.app.inner.host;
    let plan = host
        .services
        .control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
        .unwrap();
    let mut target = plan.targets[0].clone();
    assert!(matches!(
        host.admit_credential(&target, &Bytes::new(), SettleMode::OnResponse)
            .await,
        Err(CoreError::QuotaExceeded)
    ));
    input.quota_daily = Some(Decimal::ONE);
    host.services.store.update_quota(id, &input).await.unwrap();
    fixture.app.reload().await.unwrap();
    target.upstream_model = "no-price".into();
    assert!(
        matches!(host.admit_credential(&target, &Bytes::new(), SettleMode::OnResponse).await, Err(CoreError::Internal(message)) if message.contains("requires model pricing"))
    );
    host.admit_credential(&target, &Bytes::new(), SettleMode::Free)
        .await
        .unwrap();
}
