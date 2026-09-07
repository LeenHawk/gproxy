use bytes::Bytes;
use gproxy_core::{ControlPlane, CoreError, Host, ResponseBody, RoutingMode};
use gproxy_store::records::{PermissionInput, ProviderInput, ProviderModelInput, RouteMemberInput};
use serde_json::json;

use super::setup;
use crate::ControlMutation;

#[tokio::test]
async fn partial_permissions_filter_catalogues_and_route_candidates() {
    let fixture = setup::fixture().await;
    let app = &fixture.app;
    let host = &app.inner.host;
    let store = &host.services.store;
    let control = &host.services.control;
    let request = setup::request("permission-route", "hi", &fixture.client_key);
    let identity = host.authenticate(&request).await.unwrap();
    let permission = control.current().permissions[0].id;
    let other = setup::id(
        app.mutate(ControlMutation::Provider(ProviderInput {
            name: "other".into(),
            label: None,
            channel: "openai".into(),
            settings: json!({"auto_refresh_models":false}),
            credential_strategy: "round_robin".into(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        }))
        .await
        .unwrap(),
    );
    app.mutate(ControlMutation::Credential {
        provider_id: other,
        label: None,
        secret: json!({"api_key":setup::random_key()}),
        enabled: true,
    })
    .await
    .unwrap();
    app.mutate(ControlMutation::RouteMember(RouteMemberInput {
        route_id: fixture.route,
        provider_id: other,
        upstream_model: "other-model".into(),
        tier: 0,
        weight: 100,
        enabled: true,
    }))
    .await
    .unwrap();
    for provider in [fixture.provider, other] {
        store
            .update_provider(
                provider,
                &ProviderInput {
                    name: if provider == other {
                        "other"
                    } else {
                        "provider"
                    }
                    .into(),
                    label: None,
                    channel: "openai".into(),
                    settings: json!({"auto_refresh_models":false}),
                    credential_strategy: "round_robin".into(),
                    proxy_url: None,
                    tls_fingerprint: None,
                    enabled: true,
                },
            )
            .await
            .unwrap();
        store
            .insert_provider_model(&ProviderModelInput {
                provider_id: provider,
                model_id: "model".into(),
                display_name: None,
                variants: None,
                context_window: None,
                max_output_tokens: None,
                thinking_supported: None,
                thinking_adaptive_supported: None,
                thinking_enabled_supported: None,
                metadata: Default::default(),
                enabled: true,
            })
            .await
            .unwrap();
    }
    for group in [None, Some("generate_content"), Some("models")] {
        store
            .update_permission(
                permission,
                &PermissionInput {
                    subject_kind: "user".into(),
                    subject_id: identity.user_id,
                    provider_id: Some(fixture.provider),
                    operation_group: group.map(str::to_owned),
                    allowed: true,
                },
            )
            .await
            .unwrap();
        app.reload().await.unwrap();
        let plan = control
            .resolve(Some("public-model"), &request.mode, None)
            .unwrap();
        assert_eq!(plan.targets.len(), 2);
        assert!(control.catalogue_visible(&identity, Some("public-model"), &request.mode));
        let admitted = host
            .admit(
                &identity,
                &request,
                Some(super::generation_operation()),
                &plan,
            )
            .await;
        if group == Some("models") {
            assert!(matches!(admitted, Err(CoreError::Forbidden(_))));
        } else {
            let admitted = admitted.unwrap();
            assert_eq!(admitted.targets.len(), 1);
            assert_eq!(admitted.targets[0].provider.id, fixture.provider);
            assert_eq!(admitted.budget.max_attempts, plan.budget.max_attempts);
            let denied = plan
                .targets
                .iter()
                .find(|target| target.provider.id == other)
                .unwrap();
            assert!(matches!(
                host.admit_retry(
                    &request.request_id,
                    denied,
                    &request.body,
                    gproxy_protocol::SettleMode::OnResponse
                )
                .await,
                Err(CoreError::Forbidden(_))
            ));
            host.finish_admission(&request.request_id, None).await;
        }
        let mut list = request.clone();
        list.request_id = format!("list-{}", group.unwrap_or("all"));
        list.method = http::Method::GET;
        list.path = "/v1/models".into();
        list.body = Bytes::new();
        let outcome = app.execute(list.clone()).await.unwrap();
        assert_eq!(outcome.status, http::StatusCode::OK);
        let ResponseBody::Full(body) = outcome.body else {
            panic!("buffered catalogue")
        };
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let ids = body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["provider/model", "public-model"]);
        list.mode = RoutingMode::Scoped {
            provider: "other".into(),
        };
        assert!(matches!(
            app.execute(list).await,
            Err(CoreError::Forbidden(_))
        ));
        let mut direct = request.clone();
        direct.body = Bytes::from(
            json!({"model":"other/model","messages":[{"role":"user","content":"hi"}]}).to_string(),
        );
        assert!(matches!(
            app.execute(direct).await,
            Err(CoreError::Forbidden(_))
        ));
    }
    // A deny is local to its matching provider, and overrides inherited allows.
    store
        .update_permission(
            permission,
            &PermissionInput {
                subject_kind: "user".into(),
                subject_id: identity.user_id,
                provider_id: None,
                operation_group: None,
                allowed: true,
            },
        )
        .await
        .unwrap();
    app.mutate(ControlMutation::Permission(PermissionInput {
        subject_kind: "user_key".into(),
        subject_id: identity.user_key_id,
        provider_id: Some(other),
        operation_group: Some("generate_content".into()),
        allowed: false,
    }))
    .await
    .unwrap();
    let plan = control
        .resolve(Some("public-model"), &request.mode, None)
        .unwrap();
    let admitted = host
        .admit(
            &identity,
            &request,
            Some(super::generation_operation()),
            &plan,
        )
        .await
        .unwrap();
    assert_eq!(admitted.targets.len(), 1);
    assert_eq!(admitted.targets[0].provider.id, fixture.provider);
    host.finish_admission(&request.request_id, None).await;
    app.mutate(ControlMutation::Permission(PermissionInput {
        subject_kind: "user_key".into(),
        subject_id: identity.user_key_id,
        provider_id: None,
        operation_group: Some("generate_content".into()),
        allowed: false,
    }))
    .await
    .unwrap();
    assert!(matches!(
        host.admit(
            &identity,
            &request,
            Some(super::generation_operation()),
            &plan
        )
        .await,
        Err(CoreError::Forbidden(_))
    ));
}

#[tokio::test]
async fn execution_skips_unauthorized_first_candidate_without_spending_attempt_budget() {
    use futures_util::StreamExt as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let fixture = setup::fixture().await;
    let host = &fixture.app.inner.host;
    let control = &host.services.control;
    let mut request = setup::request("authorized-egress", "hi", &fixture.client_key);
    request.body = Bytes::from(
        json!({"model":"public-model","messages":[{"role":"user","content":"hi"}]}).to_string(),
    );
    let identity = host.authenticate(&request).await.unwrap();
    let permission = control.current().permissions[0].id;
    host.services
        .store
        .update_permission(
            permission,
            &PermissionInput {
                subject_kind: "user".into(),
                subject_id: identity.user_id,
                provider_id: Some(fixture.provider),
                operation_group: Some("generate_content".into()),
                allowed: true,
            },
        )
        .await
        .unwrap();
    fixture.app.reload().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let size = socket.read(&mut buffer).await.unwrap();
            assert_ne!(size, 0);
            received.extend_from_slice(&buffer[..size]);
            if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&received[..end]).unwrap();
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if received.len() >= end + 4 + length {
                    assert!(headers.starts_with("POST /allowed/v1/chat/completions "));
                    break;
                }
            }
        }
        let body = r#"{"id":"response","object":"chat.completion","created":1,"model":"upstream-model","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let mut plan = control
        .resolve(Some("public-model"), &request.mode, None)
        .unwrap();
    plan.targets[0].provider.settings = json!({"base_url":format!("http://{address}/allowed")});
    let mut denied = plan.targets[0].clone();
    denied.provider.id = -1;
    denied.provider.settings = json!({"base_url":format!("http://{address}/denied")});
    plan.targets.insert(0, denied);
    assert_eq!(plan.budget.max_attempts, 1);
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        fixture
            .app
            .inner
            .core
            .execute_planned(control, request, plan),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(outcome.status, http::StatusCode::OK);
    match outcome.body {
        ResponseBody::Full(body) => assert!(!body.is_empty()),
        ResponseBody::Stream(mut stream) => {
            while let Some(chunk) = stream.next().await {
                chunk.unwrap();
            }
        }
        ResponseBody::WebSocket(_) => panic!("HTTP response expected"),
    }
    upstream.await.unwrap();
}
