use gproxy_core::{ControlPlane, Host};
use rust_decimal::Decimal;
use serde_json::json;

use super::setup;

#[tokio::test]
async fn tokenizer_auth_is_sealed_revealable_and_clearable() {
    use gproxy_admin::State as _;

    let setup::Fixture { app, .. } = setup::fixture().await;
    let token = format!("tokenizer-auth-{}", std::process::id());
    assert!(!app.tokenizer_auth().await.expect("auth state"));

    assert!(
        app.update_tokenizer_auth(Some(&token))
            .await
            .expect("set auth")
    );
    assert_eq!(
        app.reveal_tokenizer_auth().await.expect("reveal auth"),
        token
    );
    let envelope = app
        .inner
        .host
        .services
        .store
        .tokenizer_auth("hugging_face")
        .await
        .expect("read auth")
        .expect("stored auth");
    assert!(
        !envelope
            .ciphertext
            .windows(token.len())
            .any(|window| window == token.as_bytes())
    );

    assert!(!app.update_tokenizer_auth(None).await.expect("clear auth"));
    assert!(app.reveal_tokenizer_auth().await.is_err());
}

#[tokio::test]
async fn tokenizer_admin_actions_ignore_automatic_download_policy() {
    use gproxy_admin::State as _;

    let setup::Fixture { app, .. } = setup::fixture().await;
    app.inner
        .host
        .services
        .store
        .put_tokenizer_vocab(
            "local-vocab",
            "owner/model",
            include_bytes!(
                "../../../gproxy-tokenize/assets/tokenizers/deepseek-v4-pro.tokenizer.json"
            ),
        )
        .await
        .expect("seed tokenizer");

    let vocab = app
        .fetch_tokenizer_vocab("local-vocab", "owner/model")
        .await
        .expect("manual fetch should remain available");
    assert_eq!(vocab.name, "local-vocab");
    assert_eq!(vocab.repository, "owner/model");

    app.delete_tokenizer_vocab("local-vocab")
        .await
        .expect("manual delete should remain available");
    assert_eq!(
        app.inner
            .host
            .services
            .store
            .tokenizer_vocab("local-vocab")
            .await
            .expect("read tokenizer"),
        None
    );
}

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
        .expect("second provider"),
    );
    app.mutate(crate::ControlMutation::Credential {
        provider_id: provider,
        label: None,
        secret: json!({"api_key": setup::random_key()}),
        enabled: true,
    })
    .await
    .expect("second credential");
    app.mutate(crate::ControlMutation::RouteMember(
        gproxy_store::records::RouteMemberInput {
            route_id: route,
            provider_id: provider,
            upstream_model: "gpt-4o-mini".into(),
            tier: 1,
            weight: 100,
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
        .resolve(
            Some("public-model"),
            &gproxy_core::RoutingMode::Aggregated,
            None,
        )
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
