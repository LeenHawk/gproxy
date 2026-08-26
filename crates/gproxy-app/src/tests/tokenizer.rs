use gproxy_core::{ControlPlane, Host};
use rust_decimal::Decimal;
use serde_json::json;

use super::setup;

#[tokio::test]
async fn admission_prices_each_alias_resolved_target_with_its_tokenizer() {
    let setup::Fixture {
        app,
        route,
        quota,
        client_key,
        ..
    } = setup::fixture().await;
    let provider = setup::id(
        app.mutate(crate::ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "second-provider".into(),
                channel: "openai".into(),
                settings: json!({}),
                tls_fingerprint: None,
                enabled: true,
            },
        ))
        .await
        .expect("second provider"),
    );
    let credential = setup::id(
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
            credential_id: Some(credential),
            upstream_model: "gpt-4o-mini".into(),
            priority: 1,
            enabled: true,
        },
    ))
    .await
    .expect("second route member");
    let rule = setup::id(
        app.mutate(crate::ControlMutation::PriceRule(
            gproxy_store::records::PriceRuleInput {
                provider_id: Some(provider),
                model_pattern: "gpt-4o-mini".into(),
                tiers: None,
                priority: 0,
                enabled: true,
            },
        ))
        .await
        .expect("second price rule"),
    );
    app.mutate(crate::ControlMutation::PriceRate(
        gproxy_store::records::PriceRateInput {
            rule_id: rule,
            metric: "input_tokens".into(),
            unit_size: 1,
            price: Decimal::new(2, 2),
            conditions: None,
            priority: 0,
        },
    ))
    .await
    .expect("second input price");

    let host = &app.inner.host;
    let plan = host
        .services
        .control
        .resolve(Some("public-model"), &gproxy_core::RoutingMode::Aggregated)
        .expect("resolved plan");
    assert_eq!(
        plan.targets
            .iter()
            .map(|target| target.upstream_model.as_str())
            .collect::<Vec<_>>(),
        ["upstream-model", "gpt-4o-mini"]
    );
    let request = setup::request("tokenizer-plan", "measure target model", &client_key);
    let expected = plan
        .targets
        .iter()
        .filter_map(|target| {
            let pricing = host
                .services
                .control
                .pricing(&target.provider, &target.upstream_model)?;
            let tokens = gproxy_tokenize::count(
                &target.upstream_model,
                &request.body,
                target.provider.settings.get("tokenizer_map"),
                &host.services.tokenizers,
            );
            Some(pricing.cost(&gproxy_core::NormalizedUsage {
                input_tokens: tokens,
                ..Default::default()
            }))
        })
        .max()
        .and_then(gproxy_core::usage::cost_to_micros)
        .expect("estimated cost");
    let legacy_tokens = gproxy_core::usage::estimate_input_tokens(&request.body);
    assert_ne!(expected, legacy_tokens as i64 * 20_000);

    let identity = host.authenticate(&request).await.expect("identity");
    host.admit(
        &identity,
        &request,
        Some(super::generation_operation()),
        &plan,
    )
    .await
    .expect("admission");
    let windows = app.quota_windows().await.expect("quota windows");
    for window in windows.iter().filter(|window| window.quota_id == quota) {
        assert_eq!(setup::counter(host, window.id).await, expected);
    }
    host.finish_admission(&request.request_id, None).await;
}
