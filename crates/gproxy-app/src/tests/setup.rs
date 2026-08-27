use base64::Engine as _;
use bytes::Bytes;
use gproxy_core::CacheBackend;
use http::{HeaderMap, HeaderValue, Method};
use rust_decimal::Decimal;
use serde_json::json;

use crate::{App, AppHandle, Config, ControlMutation, MutationResult};

pub(super) struct Fixture {
    pub app: AppHandle,
    pub provider: i64,
    pub credential: i64,
    pub route: i64,
    pub quota: i64,
    pub client_key: String,
    pub _directory: tempfile::TempDir,
}

pub(super) async fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("app tempdir");
    let mut master_key = [0_u8; 32];
    getrandom::fill(&mut master_key).expect("master key randomness");
    let upstream_key = random_key();
    let client_key = random_key();
    let config = Config::sqlite(
        "127.0.0.1:0".parse().unwrap(),
        directory.path().to_path_buf(),
        crate::MasterKeyConfig::new(Some(master_key)),
    );
    let app = App::start(config).await.expect("start app");
    let provider = id(app
        .mutate(ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "provider".into(),
                label: None,
                channel: "openai".into(),
                settings: json!({}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
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
            tier: 0,
            weight: 100,
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
            quota_total: Decimal::ONE,
            quota_daily: None,
            quota_weekly: None,
            quota_monthly: None,
            quota_5h: Some(Decimal::new(3, 1)),
            quota_7d: None,
            enabled: true,
        }))
        .await
        .expect("quota"));
    let price_rule = id(app
        .mutate(ControlMutation::PriceRule(
            gproxy_store::records::PriceRuleInput {
                provider_id: Some(provider),
                model_pattern: "upstream-model".into(),
                tiers: None,
                priority: 0,
                enabled: true,
            },
        ))
        .await
        .expect("price rule"));
    app.mutate(ControlMutation::PriceRate(
        gproxy_store::records::PriceRateInput {
            rule_id: price_rule,
            metric: "input_tokens".into(),
            unit_size: 1,
            price: Decimal::new(1, 2),
            conditions: None,
            priority: 0,
        },
    ))
    .await
    .expect("input price");
    Fixture {
        app,
        provider,
        credential,
        route,
        quota,
        client_key,
        _directory: directory,
    }
}

pub(super) fn random_key() -> String {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).expect("API key randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn id(result: MutationResult) -> i64 {
    let MutationResult::Id(id) = result else {
        panic!("mutation returned no id")
    };
    id
}

pub(super) fn request(id: &str, input: &str, api_key: &str) -> gproxy_core::RequestCtx {
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

pub(super) async fn counter(host: &crate::host::AppHost, window_id: i64) -> i64 {
    let value = host
        .services
        .cache
        .get(&format!("gproxy:quota-pending:{window_id}"))
        .await
        .expect("cache")
        .expect("quota counter");
    i64::from_be_bytes(value.try_into().expect("counter bytes"))
}
