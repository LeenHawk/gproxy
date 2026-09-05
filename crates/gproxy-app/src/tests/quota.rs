use gproxy_core::{ControlPlane, Host};
use rust_decimal::Decimal;

use super::setup;

const QUOTA_INPUT: &str = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty";

#[tokio::test]
async fn admission_refunds_reconciles_and_leaves_no_failed_reservation() {
    let setup::Fixture {
        app,
        provider,
        credential,
        route: _,
        quota,
        client_key,
        _directory,
    } = setup::fixture().await;

    let host = &app.inner.host;
    let plan = host
        .services
        .control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
        .expect("plan");
    let first = setup::request("refund", QUOTA_INPUT, &client_key);
    let identity = host.authenticate(&first).await.expect("authenticate");
    let operation = super::generation_operation();
    host.admit(&identity, &first, Some(operation), &plan)
        .await
        .expect("first admission");
    assert!(app.admission_pending(&first.request_id).await.unwrap());
    let overlap = setup::request("overlap", QUOTA_INPUT, &client_key);
    assert!(matches!(
        host.admit(&identity, &overlap, Some(operation), &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&overlap.request_id).await.unwrap());
    host.finish_admission(&first.request_id, None).await;
    assert!(!app.admission_pending(&first.request_id).await.unwrap());

    let second = setup::request("settle", QUOTA_INPUT, &client_key);
    host.admit(&identity, &second, Some(operation), &plan)
        .await
        .expect("second admission");
    let settlement = gproxy_core::Settlement {
        upstream_started_at_ms: None,
        request_id: second.request_id.clone(),
        provider_id: provider,
        credential_id: gproxy_core::CredentialId(credential),
        upstream_model: "upstream-model".into(),
        usage: gproxy_core::NormalizedUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        cost: Decimal::new(2, 1),
        source: gproxy_core::UsageSource::Upstream,
        ended: gproxy_core::Ended::Complete,
        latency_ms: 1,
    };
    tokio::join!(
        host.finish_admission(&second.request_id, Some(&settlement)),
        host.finish_admission(&second.request_id, Some(&settlement)),
    );
    assert!(!app.admission_pending(&second.request_id).await.unwrap());

    let windows: Vec<_> = app
        .quota_windows()
        .await
        .unwrap()
        .into_iter()
        .filter(|window| window.quota_id == quota)
        .collect();
    assert_eq!(windows.len(), 2);
    assert!(windows.iter().any(|window| window.reset_at.is_none()));
    assert!(windows.iter().any(|window| window.reset_at.is_some()));
    for window in &windows {
        assert_eq!(window.cost_used, Decimal::new(2, 1));
        assert_eq!(setup::counter(host, window.id).await, 0);
    }

    let rejected = setup::request("reject", QUOTA_INPUT, &client_key);
    assert!(matches!(
        host.admit(&identity, &rejected, Some(operation), &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&rejected.request_id).await.unwrap());
    for window in &windows {
        assert_eq!(setup::counter(host, window.id).await, 0);
    }
}
