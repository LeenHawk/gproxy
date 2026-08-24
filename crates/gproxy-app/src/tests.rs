mod pressure;
mod setup;

use bytes::Bytes;
use gproxy_core::{CacheBackend, ControlPlane, Host};
use http::{HeaderMap, HeaderValue, Method};
use rust_decimal::Decimal;
use serde_json::json;

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
        .resolve(Some("public-model"), &gproxy_core::RoutingMode::Aggregated)
        .expect("plan");
    let first = request("refund", "hi", &client_key);
    let identity = host.authenticate(&first).await.expect("authenticate");
    let operation = gproxy_protocol::OperationKey::content(
        gproxy_protocol::Operation::GenerateContent,
        gproxy_protocol::ContentGenerationKind::OpenAiChat,
    );
    host.admit(&identity, &first, Some(operation), &plan)
        .await
        .expect("first admission");
    assert!(app.admission_pending(&first.request_id).await.unwrap());
    let overlap = request("overlap", "hi", &client_key);
    assert!(matches!(
        host.admit(&identity, &overlap, Some(operation), &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&overlap.request_id).await.unwrap());
    host.finish_admission(&first.request_id, None).await;
    assert!(!app.admission_pending(&first.request_id).await.unwrap());

    let second = request("settle", "hi", &client_key);
    host.admit(&identity, &second, Some(operation), &plan)
        .await
        .expect("second admission");
    let settlement = gproxy_core::Settlement {
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
        assert_eq!(counter(host, window.id).await, 0);
    }

    let rejected = request("reject", "hi", &client_key);
    assert!(matches!(
        host.admit(&identity, &rejected, Some(operation), &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&rejected.request_id).await.unwrap());
    for window in &windows {
        assert_eq!(counter(host, window.id).await, 0);
    }
}

fn request(id: &str, input: &str, api_key: &str) -> gproxy_core::RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}")).expect("authorization header"),
    );
    gproxy_core::RequestCtx {
        request_id: format!("request-{id}"),
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers,
        body: Bytes::from(json!({"model": "public-model", "input": input}).to_string()),
        upgrade: false,
        mode: gproxy_core::RoutingMode::Aggregated,
    }
}

async fn counter(host: &crate::host::AppHost, window_id: i64) -> i64 {
    let value = host
        .services
        .cache
        .get(&format!("gproxy:quota-pending:{window_id}"))
        .await
        .expect("cache")
        .expect("quota counter");
    i64::from_be_bytes(value.try_into().expect("counter bytes"))
}
