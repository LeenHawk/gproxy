use base64::Engine as _;
use bytes::Bytes;
use gproxy_core::{CacheBackend, ControlPlane, Host};
use http::{HeaderMap, HeaderValue, Method};
use rust_decimal::Decimal;
use serde_json::json;

use crate::{App, Config, ControlMutation, MutationResult};

#[tokio::test]
async fn admission_refunds_reconciles_and_leaves_no_failed_reservation() {
    let directory = tempfile::tempdir().expect("app tempdir");
    let mut master_key = [0_u8; 32];
    getrandom::fill(&mut master_key).expect("master key randomness");
    let upstream_key = random_key();
    let client_key = random_key();
    let config = Config::from_toml(&format!(
        "listen_addr = \"127.0.0.1:0\"\ndata_dir = {:?}\nstore_backend = \"sqlite\"\nsecret_key = \"{}\"\n",
        directory.path().display().to_string(),
        base64::engine::general_purpose::STANDARD.encode(master_key),
    ))
    .expect("config");
    let app = App::start(config).await.expect("start app");
    let provider = id(app
        .mutate(ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "provider".into(),
                channel: "openai".into(),
                settings: json!({}),
                enabled: true,
            },
        ))
        .await
        .expect("provider"));
    let credential = id(app
        .mutate(ControlMutation::Credential {
            provider_id: provider,
            label: None,
            secret: json!({"api_key": upstream_key}),
            enabled: true,
        })
        .await
        .expect("credential"));
    let route = id(app
        .mutate(ControlMutation::Route(gproxy_store::records::RouteInput {
            name: "route".into(),
            max_attempts: 1,
            enabled: true,
        }))
        .await
        .expect("route"));
    app.mutate(ControlMutation::RouteMember(
        gproxy_store::records::RouteMemberInput {
            route_id: route,
            provider_id: provider,
            credential_id: Some(credential),
            upstream_model: "upstream-model".into(),
            priority: 0,
            enabled: true,
        },
    ))
    .await
    .expect("route member");
    app.mutate(ControlMutation::ExposedModel(
        gproxy_store::records::ExposedModelInput {
            name: "public-model".into(),
            route_id: route,
            enabled: true,
        },
    ))
    .await
    .expect("model");
    let user = id(app
        .mutate(ControlMutation::User(gproxy_store::records::UserInput {
            name: "user".into(),
            organization_id: None,
            team_id: None,
            enabled: true,
        }))
        .await
        .expect("user"));
    let user_key = id(app
        .mutate(ControlMutation::UserKey {
            user_id: user,
            api_key: client_key.clone(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .expect("user key"));
    app.mutate(ControlMutation::Permission(
        gproxy_store::records::PermissionInput {
            subject_kind: "user_key".into(),
            subject_id: user_key,
            provider_id: None,
            operation_group: Some("generate_content".into()),
            allowed: true,
        },
    ))
    .await
    .expect("permission");
    let quota = id(app
        .mutate(ControlMutation::Quota(gproxy_store::records::QuotaInput {
            subject_kind: "user_key".into(),
            subject_id: user_key,
            token_limit: 100,
            window_seconds: 3_600,
        }))
        .await
        .expect("quota"));

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
    host.finish_admission(&first.request_id, None).await;
    assert!(!app.admission_pending(&first.request_id).await.unwrap());

    let second = request("settle", "hi", &client_key);
    host.admit(&identity, &second, Some(operation), &plan)
        .await
        .expect("second admission");
    host.finish_admission(
        &second.request_id,
        Some(&gproxy_core::Settlement {
            request_id: second.request_id.clone(),
            provider_id: provider,
            credential_id: gproxy_core::CredentialId(credential),
            upstream_model: "upstream-model".into(),
            usage: gproxy_core::NormalizedUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            },
            cost: Decimal::new(2, 5),
            source: gproxy_core::UsageSource::Upstream,
            ended: gproxy_core::Ended::Complete,
            latency_ms: 1,
        }),
    )
    .await;
    assert!(!app.admission_pending(&second.request_id).await.unwrap());

    let start = window_start(3_600);
    assert_eq!(counter(host, quota, start).await, 15);
    assert_eq!(
        app.quota_window(quota, start)
            .await
            .unwrap()
            .expect("durable quota")
            .used_tokens,
        15
    );

    let rejected = request("reject", &"x".repeat(300), &client_key);
    assert!(matches!(
        host.admit(&identity, &rejected, Some(operation), &plan)
            .await,
        Err(gproxy_core::CoreError::QuotaExceeded)
    ));
    assert!(!app.admission_pending(&rejected.request_id).await.unwrap());
    assert_eq!(counter(host, quota, start).await, 15);
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

fn random_key() -> String {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).expect("API key randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

async fn counter(host: &crate::host::AppHost, quota: i64, start: i64) -> i64 {
    let value = host
        .services
        .cache
        .get(&format!("gproxy:quota:{quota}:{start}"))
        .await
        .expect("cache")
        .expect("quota counter");
    i64::from_be_bytes(value.try_into().expect("counter bytes"))
}

fn window_start(seconds: i64) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("current time")
        .as_secs() as i64;
    now - now.rem_euclid(seconds)
}

fn id(result: MutationResult) -> i64 {
    let MutationResult::Id(id) = result else {
        panic!("mutation returned no id")
    };
    id
}
