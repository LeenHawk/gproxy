use gproxy_core::{ControlPlane, Host};
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
        client_key,
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
    let before = resolve_credentials(&app);
    assert_eq!(before, vec![credential, second]);

    let now = unix_now();
    let cycle = app
        .observe_credential_quota_cycle(CredentialQuotaObservation {
            unit: Some("requests".into()),
            reset_behavior: gproxy_core::QuotaResetBehavior::Periodic,
            scope: gproxy_core::QuotaScope::All,
            sample: gproxy_core::QuotaSample {
                source: gproxy_core::QuotaSampleSource::Unknown,
                started_at_ms: now * 1000,
                received_at_ms: now * 1000,
            },
            credential_id: credential,
            window_key: "five-hour".into(),
            label: None,
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

    let request = setup::request("cycle-view", "hi", &client_key);
    let identity = app
        .inner
        .host
        .authenticate(&request)
        .await
        .expect("identity");
    let plan = app
        .inner
        .host
        .services
        .control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
        .expect("plan");
    let windows = app
        .inner
        .host
        .surface_usage(
            &identity,
            &plan.targets[0].provider,
            gproxy_core::CredentialId(credential),
        )
        .quota_windows()
        .await
        .expect("credential quota view");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].key, "five-hour");
    assert_eq!(windows[0].used_percent, Some(Decimal::from(95)));
    assert_eq!(windows[0].reset_at, Some(now + 18_000));
}

fn resolve_credentials(app: &crate::AppHandle) -> Vec<i64> {
    app.inner
        .host
        .services
        .control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
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
