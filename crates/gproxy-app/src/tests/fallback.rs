use gproxy_core::{ControlPlane, Host, UsageSink};
use rust_decimal::Decimal;

use super::setup;

#[tokio::test]
async fn fallback_rows_keep_caller_identity_and_total_quota_cost_is_recorded_once() {
    for logging in [true, false] {
        let fixture = setup::fixture().await;
        super::setting(&fixture.app, "enable_usage", serde_json::json!(logging)).await;
        let host = &fixture.app.inner.host;
        let request = setup::request("fallback-ledger", "one", &fixture.client_key);
        let identity = host.authenticate(&request).await.unwrap();
        let plan = host
            .services
            .control
            .resolve(
                Some("public-model"),
                &gproxy_core::RoutingMode::Aggregated,
                None,
            )
            .unwrap();
        let key = gproxy_protocol::OperationKey::content(
            gproxy_protocol::Operation::GenerateContent,
            gproxy_protocol::ContentGenerationKind::OpenAiChat,
        );
        host.admit(&identity, &request, Some(key), &plan)
            .await
            .unwrap();
        host.admit_retry(
            &request.request_id,
            &plan.targets[0],
            &request.body,
            gproxy_protocol::SettleMode::OnResponse,
        )
        .await
        .unwrap();
        let started = Some(
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        );
        let settlement = gproxy_core::Settlement {
            upstream_started_at_ms: started,
            request_id: request.request_id.clone(),
            provider_id: fixture.provider,
            credential_id: plan.targets[0].credential,
            upstream_model: "alternate-model".into(),
            usage: Default::default(),
            cost: Decimal::new(2, 1),
            source: gproxy_core::UsageSource::Upstream,
            ended: gproxy_core::Ended::Complete,
            latency_ms: 1,
            attempts: vec![
                gproxy_core::SettledAttempt {
                    upstream_model: "upstream-model".into(),
                    usage: gproxy_core::NormalizedUsage {
                        input_tokens: 100,
                        ..Default::default()
                    },
                    cost: Decimal::ZERO,
                    billable: false,
                    source: gproxy_core::UsageSource::Upstream,
                    started_at_ms: started,
                },
                gproxy_core::SettledAttempt {
                    upstream_model: "alternate-model".into(),
                    usage: gproxy_core::NormalizedUsage {
                        input_tokens: 8,
                        output_tokens: 5,
                        ..Default::default()
                    },
                    cost: Decimal::new(2, 1),
                    billable: true,
                    source: gproxy_core::UsageSource::Upstream,
                    started_at_ms: started,
                },
            ],
        };
        host.record(&settlement).await;
        host.record(&settlement).await;
        host.finish_admission(&request.request_id, Some(&settlement))
            .await;
        assert_eq!(
            host.services.store.usage_count().await.unwrap(),
            if logging { 2 } else { 0 }
        );
        if logging {
            let first = host
                .services
                .store
                .usage_by_request(&request.request_id)
                .await
                .unwrap()
                .unwrap()
                .usage;
            let second = host
                .services
                .store
                .usage_by_request(&format!("{}:attempt:1", request.request_id))
                .await
                .unwrap()
                .unwrap()
                .usage;
            assert_eq!(first.input_tokens, 100);
            assert_eq!(first.cost, Decimal::ZERO);
            assert_eq!(second.upstream_model, "alternate-model");
            assert_eq!(second.user_key_id, Some(identity.user_key_id));
            assert_eq!(second.user_id, Some(identity.user_id));
            assert_eq!(second.dimensions["parent_request_id"], request.request_id);
            host.services
                .store
                .begin_request_log(&gproxy_store::records::RequestLogInput {
                    request_id: request.request_id.clone(),
                    at: 1,
                    method: "POST".into(),
                    path: "/v1/messages".into(),
                    query: None,
                    client_ip: None,
                    request_headers: None,
                    request_body: None,
                })
                .await
                .unwrap();
            for model in ["upstream-model", "alternate-model"] {
                host.services
                    .store
                    .record_capture(&gproxy_store::records::CaptureInput {
                        request_id: request.request_id.clone(),
                        at: 1,
                        provider_id: Some(fixture.provider),
                        credential_id: Some(fixture.credential),
                        upstream_url: Some(format!("https://upstream.example/{model}")),
                        request_method: Some("POST".into()),
                        request_headers: None,
                        response_status: Some(200),
                        response_headers: None,
                        request_body: None,
                        response_body: None,
                    })
                    .await
                    .unwrap();
            }
            let detail = host
                .services
                .store
                .log_detail(&second.request_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(detail.downstream.input.request_id, request.request_id);
            assert_eq!(detail.upstream.len(), 2);
        }
        for window in fixture
            .app
            .quota_windows()
            .await
            .unwrap()
            .into_iter()
            .filter(|window| window.quota_id == fixture.quota)
        {
            assert_eq!(window.cost_used, Decimal::new(2, 1));
            assert_eq!(setup::counter(host, window.id).await, 0);
        }
    }
}

#[tokio::test]
async fn fallback_admission_rejects_extra_spend_and_rolls_back_its_reservation() {
    let fixture = setup::fixture().await;
    let host = &fixture.app.inner.host;
    let request = setup::request("fallback-quota", "one", &fixture.client_key);
    let identity = host.authenticate(&request).await.unwrap();
    let plan = host
        .services
        .control
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
        .unwrap();
    let key = gproxy_protocol::OperationKey::content(
        gproxy_protocol::Operation::GenerateContent,
        gproxy_protocol::ContentGenerationKind::OpenAiChat,
    );
    host.admit(&identity, &request, Some(key), &plan)
        .await
        .unwrap();
    let windows = fixture.app.quota_windows().await.unwrap();
    let mut before = Vec::new();
    for window in &windows {
        before.push(setup::counter(host, window.id).await);
    }
    let large = setup::request("ignored", &"input ".repeat(100), &fixture.client_key);
    assert!(matches!(
        host.admit_retry(
            &request.request_id,
            &plan.targets[0],
            &large.body,
            gproxy_protocol::SettleMode::OnResponse
        )
        .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    for (index, window) in windows.iter().enumerate() {
        assert_eq!(setup::counter(host, window.id).await, before[index]);
    }
    host.finish_admission(&request.request_id, None).await;
    for window in &windows {
        assert_eq!(setup::counter(host, window.id).await, 0);
    }
}
