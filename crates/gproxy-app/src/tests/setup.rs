use base64::Engine as _;
use bytes::Bytes;
use gproxy_core::{CacheBackend, ControlPlane};
use http::{HeaderMap, HeaderValue, Method};
use rust_decimal::Decimal;
use serde_json::json;

use crate::{App, AppHandle, Config, ControlMutation, MutationResult};

pub(super) use super::v2_schema::{v2_database, v2_seal};

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
            strategy: Default::default(),
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
            password_hash: None,
            enabled: true,
            is_admin: false,
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
            model_pattern: None,
            allowed: true,
        },
    ))
    .await
    .expect("permission");
    let quota = id(app
        .mutate(ControlMutation::Quota(gproxy_store::records::QuotaInput {
            subject_kind: "user_key".into(),
            subject_id: user_key,
            quota_total: Some(Decimal::ONE),
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
        client_ip: None,
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers,
        body: Bytes::from(json!({"model": "public-model", "input": input}).to_string()),
        upgrade: false,
        force_model_refresh: false,
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

#[tokio::test]
async fn v2_digest_crossing_authenticates_the_unreissued_key() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", random_key());
    let mut source_key = [0_u8; 32];
    let mut target_key = [0_u8; 32];
    getrandom::fill(&mut source_key).unwrap();
    getrandom::fill(&mut target_key).unwrap();
    let stored_key = serde_json::to_string(&v2_seal(&json!(api_key), source_key)).unwrap();
    let stored_credential = v2_seal(&json!({"api_key": random_key()}), source_key);
    v2_database(
        source.path(),
        &api_key,
        &stored_key,
        &stored_credential,
        true,
    );
    let config = Config::sqlite(
        "127.0.0.1:0".parse().unwrap(),
        target.path().into(),
        crate::MasterKeyConfig::new(Some(target_key)),
    );
    let report = crate::migrate_from_v2(
        &config,
        crate::V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: Some(base64::engine::general_purpose::STANDARD.encode(source_key)),
            apply: true,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.applied, "{report}");

    let app = App::start(config).await.unwrap();
    assert!(
        app.inner
            .host
            .services
            .control
            .current()
            .routing_rules
            .iter()
            .any(|rule| {
                rule.kind == "openai_chat" && rule.dest_kind.as_deref() == Some("openai_responses")
            })
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}")).unwrap(),
    );
    let identity = crate::host::authenticate_headers(&app.inner.host, &headers)
        .expect("migrated key authenticates");
    assert_eq!(identity.user_id, 1);
    assert_eq!(app.update_channel().as_deref(), Some("staging"));
    assert!(
        app.inner
            .host
            .services
            .control
            .current()
            .settings
            .iter()
            .any(
                |setting| setting.key == "enable_auto_update_check" && setting.value == json!(true)
            )
    );
    // v2 and v3 store capability at the same grain, so the rows carry across unchanged —
    // including variants, which stay named against the member's own upstream model.
    let model = app.inner.host.services.control.current().provider_models[0].clone();
    assert_eq!(model.context_window, Some(128_000));
    assert_eq!(model.max_output_tokens, Some(16_384));
    assert_eq!(model.thinking_supported, Some(true));
    assert_eq!(
        model.variants,
        Some(json!(["upstream-model-thinking-high"]))
    );
    // The catalogue is the fold: limits re-advertised, variants re-based onto the public name.
    let exposed = gproxy_core::ControlPlane::exposed_models(&app.inner.host.services.control);
    let public = exposed
        .iter()
        .find(|model| model.id == "public-model")
        .expect("public model in catalogue");
    assert_eq!(public.context_window, Some(128_000));
    assert!(
        exposed
            .iter()
            .any(|model| model.id == "public-model-thinking-high"),
        "the upstream variant suffix is re-based onto the public name"
    );
    assert_eq!(
        app.inner.host.services.control.resolve_variant(
            "public-model-thinking-high",
            &gproxy_core::RoutingMode::Aggregated,
        ),
        Some("public-model".into())
    );
}

#[tokio::test]
async fn v2_unrecoverable_key_is_reported_before_target_open() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let broken = json!({"kek_id":"local-x","wrapped_dek":"bad","nonce":"bad","ciphertext":"bad"});
    let api_key = format!("sk-{}", random_key());
    v2_database(
        source.path(),
        &api_key,
        &broken.to_string(),
        &json!({"api_key":random_key()}),
        false,
    );
    let report = crate::migrate_from_v2(
        &super::test_config(target.path(), crate::MasterKeyConfig::new(None)),
        crate::V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: None,
            apply: true,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.entity == "user_keys" && issue.row == "id=1")
    );
    assert!(!report.applied, "{report}");
    assert!(!target.path().join("gproxy.db").exists());
}

#[tokio::test]
async fn v2_dry_run_does_not_create_the_target_store() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", random_key());
    v2_database(
        source.path(),
        &api_key,
        &api_key,
        &json!({"api_key":random_key()}),
        false,
    );
    let report = crate::migrate_from_v2(
        &super::test_config(target.path(), crate::MasterKeyConfig::new(None)),
        crate::V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: None,
            apply: false,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.dry_run && report.issues.is_empty(), "{report}");
    assert!(!target.path().join("gproxy.db").exists());
}

#[tokio::test]
async fn v2_reimport_is_idempotent_and_preserves_usage_cost() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", random_key());
    v2_database(
        source.path(),
        &api_key,
        &api_key,
        &json!({"api_key":random_key()}),
        true,
    );
    let options = || crate::V2ImportOptions {
        path: source.path().join("gproxy.db"),
        source_master_key: None,
        apply: true,
        merge: false,
    };
    let config = super::test_config(target.path(), crate::MasterKeyConfig::new(None));
    assert!(
        crate::migrate_from_v2(&config, options())
            .await
            .unwrap()
            .applied
    );
    let second = crate::migrate_from_v2(&config, options()).await.unwrap();
    assert!(second.already_imported && !second.applied, "{second}");
    let store = gproxy_store::Store::open(config.backend_config())
        .await
        .unwrap();
    assert_eq!(store.usage_count().await.unwrap(), 1);
    assert_eq!(
        store
            .usage_by_request("v2-request")
            .await
            .unwrap()
            .unwrap()
            .usage
            .cost,
        "12.34".parse::<Decimal>().unwrap()
    );
}
