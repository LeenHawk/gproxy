use gproxy_core::ControlPlane;
use gproxy_store::records::{
    CredentialQuotaObservation, QuotaBoundaryConfidence, QuotaBoundarySource,
};
use rust_decimal::Decimal;
use serde_json::json;

use super::setup;

#[tokio::test]
async fn near_limit_credential_is_deprioritized() {
    let setup::Fixture {
        app,
        provider,
        credential,
        route,
        ..
    } = setup::fixture().await;
    let second = setup::id(
        app.mutate(crate::ControlMutation::Credential {
            provider_id: provider,
            label: None,
            secret: json!({"api_key": setup::random_key()}),
            enabled: true,
        })
        .await
        .expect("second credential"),
    );
    app.mutate(crate::ControlMutation::RouteMember(
        gproxy_store::records::RouteMemberInput {
            route_id: route,
            provider_id: provider,
            credential_id: Some(second),
            upstream_model: "upstream-model".into(),
            priority: 1,
            enabled: true,
        },
    ))
    .await
    .expect("second route member");

    let before = resolve_credentials(&app);
    assert_eq!(before, vec![credential, second]);

    let now = unix_now();
    let cycle = app
        .observe_credential_quota_cycle(CredentialQuotaObservation {
            credential_id: credential,
            window_key: "five-hour".into(),
            period_start: Some(now - 60),
            period_end: Some(now + 18_000),
            boundary_source: QuotaBoundarySource::Upstream,
            boundary_confidence: QuotaBoundaryConfidence::Exact,
            observed_at: now,
            upstream_used: Some(Decimal::from(95)),
            upstream_limit: Some(Decimal::from(100)),
            used_percent: Some(Decimal::from(95)),
        })
        .await
        .expect("quota observation");
    assert_eq!(cycle.metrics["requests"], json!("0"));

    assert_eq!(resolve_credentials(&app), vec![second, credential]);
}

fn resolve_credentials(app: &crate::AppHandle) -> Vec<i64> {
    app.inner
        .host
        .services
        .control
        .resolve(Some("public-model"), &gproxy_core::RoutingMode::Aggregated)
        .expect("plan")
        .targets
        .into_iter()
        .map(|target| target.credential.0)
        .collect()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
