use gproxy_app::{ControlMutation, MutationResult};
use rust_decimal::Decimal;
use serde_json::json;

pub(crate) async fn operational(
    app: &gproxy_app::AppHandle,
    upstream: std::net::SocketAddr,
    upstream_key: String,
    client_key: &str,
) -> i64 {
    let provider = id(app
        .mutate(ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "stub-openai".into(),
                channel: "openai".into(),
                settings: json!({"base_url": format!("http://{upstream}")}),
                tls_fingerprint: None,
                enabled: true,
            },
        ))
        .await
        .expect("create provider"));
    let credential = id(app
        .mutate(ControlMutation::Credential {
            provider_id: provider,
            label: None,
            secret: json!({"api_key": upstream_key}),
            enabled: true,
        })
        .await
        .expect("create credential"));
    let route = id(app
        .mutate(ControlMutation::Route(gproxy_store::records::RouteInput {
            name: "e2e-route".into(),
            max_attempts: 1,
            enabled: true,
        }))
        .await
        .expect("create route"));
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
    .expect("create route member");
    app.mutate(ControlMutation::ExposedModel(
        gproxy_store::records::ExposedModelInput {
            name: "public-model".into(),
            route_id: route,
            enabled: true,
        },
    ))
    .await
    .expect("create exposed model");
    let user = id(app
        .mutate(ControlMutation::User(gproxy_store::records::UserInput {
            name: "e2e-user".into(),
            organization_id: None,
            team_id: None,
            enabled: true,
        }))
        .await
        .expect("create user"));
    let user_key = id(app
        .mutate(ControlMutation::UserKey {
            user_id: user,
            api_key: client_key.to_owned(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .expect("create user key"));
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
    .expect("create permission");
    let quota = id(app
        .mutate(ControlMutation::Quota(gproxy_store::records::QuotaInput {
            subject_kind: "user_key".into(),
            subject_id: user_key,
            quota_total: Decimal::from(1_000),
            quota_daily: Some(Decimal::from(100)),
            quota_weekly: None,
            quota_monthly: None,
            quota_5h: None,
            quota_7d: None,
            enabled: true,
        }))
        .await
        .expect("create quota"));
    let rule = id(app
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
        .expect("create price rule"));
    for (metric, price) in [("input_tokens", 1), ("output_tokens", 2)] {
        app.mutate(ControlMutation::PriceRate(
            gproxy_store::records::PriceRateInput {
                rule_id: rule,
                metric: metric.into(),
                unit_size: 1_000_000,
                price: Decimal::from(price),
                conditions: None,
                priority: 0,
            },
        ))
        .await
        .expect("create price rate");
    }
    quota
}

fn id(result: MutationResult) -> i64 {
    let MutationResult::Id(id) = result else {
        panic!("mutation returned no id")
    };
    id
}
