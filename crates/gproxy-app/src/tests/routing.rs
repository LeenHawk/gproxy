use gproxy_core::{ControlPlane, CoreError, RoutingMode};
use serde_json::json;

use super::setup;

#[tokio::test]
async fn aggregated_provider_models_match_the_catalogue_and_provider_preprocessing() {
    let setup::Fixture {
        app,
        provider,
        route,
        ..
    } = setup::fixture().await;
    app.inner
        .host
        .services
        .store
        .update_provider(
            provider,
            &gproxy_store::records::ProviderInput {
                name: "codex".into(),
                label: None,
                channel: "openai".into(),
                settings: json!({}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    app.inner
        .host
        .services
        .store
        .insert_provider_model(&gproxy_store::records::ProviderModelInput {
            provider_id: provider,
            model_id: "gpt-5.6-sol".into(),
            display_name: None,
            variants: Some(json!(["gpt-5.6-sol-thinking-high"])),
            context_window: None,
            max_output_tokens: None,
            thinking_supported: Some(true),
            thinking_adaptive_supported: None,
            thinking_enabled_supported: None,
            metadata: Default::default(),
            enabled: true,
        })
        .await
        .unwrap();
    for (alias, target, provider_id) in [
        ("latest", "gpt-5.6-sol", Some(provider)),
        ("codex-latest", "codex/latest", None),
    ] {
        app.mutate(crate::ControlMutation::Alias(
            gproxy_store::records::AliasInput {
                alias: alias.into(),
                target: target.into(),
                provider_id,
                priority: 0,
                enabled: true,
            },
        ))
        .await
        .unwrap();
    }
    app.reload().await.unwrap();

    let control = &app.inner.host.services.control;
    let catalogue_model = control
        .provider_catalogue()
        .into_iter()
        .find(|model| model.id == "codex/gpt-5.6-sol")
        .expect("provider model is advertised");
    for requested in [
        catalogue_model.id.as_str(),
        "codex/latest",
        "codex/gpt-5.6-sol-thinking-high",
        "codex-latest",
    ] {
        assert_target(control, requested, provider, "gpt-5.6-sol");
    }

    app.mutate(crate::ControlMutation::ExposedModel(
        gproxy_store::records::ExposedModelInput {
            name: "codex/gpt-5.6-sol".into(),
            route_id: route,
            enabled: true,
        },
    ))
    .await
    .unwrap();
    assert_target(control, "codex/gpt-5.6-sol", provider, "upstream-model");
}

#[tokio::test]
async fn aggregated_provider_models_reject_unknown_and_disabled_providers() {
    let setup::Fixture { app, .. } = setup::fixture().await;
    app.mutate(crate::ControlMutation::Provider(
        gproxy_store::records::ProviderInput {
            name: "disabled".into(),
            label: None,
            channel: "openai".into(),
            settings: json!({}),
            credential_strategy: "round_robin".into(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: false,
        },
    ))
    .await
    .unwrap();

    let control = &app.inner.host.services.control;
    for (requested, provider) in [("missing/model", "missing"), ("disabled/model", "disabled")] {
        assert!(matches!(
            control.resolve(Some(requested), &RoutingMode::Aggregated, None),
            Err(CoreError::UnknownProvider(name)) if name == provider
        ));
    }
}

#[tokio::test]
async fn aggregated_models_split_only_the_provider_and_exact_routes_win() {
    let setup::Fixture {
        app,
        provider,
        route,
        ..
    } = setup::fixture().await;
    let nested_provider = setup::id(
        app.mutate(crate::ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "a".into(),
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
        .unwrap(),
    );
    app.mutate(crate::ControlMutation::Credential {
        provider_id: nested_provider,
        label: None,
        secret: json!({"api_key": setup::random_key()}),
        enabled: true,
    })
    .await
    .unwrap();

    let control = &app.inner.host.services.control;
    assert_target(control, "a/b/c", nested_provider, "b/c");

    for name in ["a/b", "a/b/c"] {
        app.mutate(crate::ControlMutation::ExposedModel(
            gproxy_store::records::ExposedModelInput {
                name: name.into(),
                route_id: route,
                enabled: true,
            },
        ))
        .await
        .unwrap();
        assert_target(control, name, provider, "upstream-model");
    }
}

fn assert_target(
    control: &impl ControlPlane,
    requested: &str,
    provider_id: i64,
    upstream_model: &str,
) {
    let plan = control
        .resolve(Some(requested), &RoutingMode::Aggregated, None)
        .expect("aggregated provider model resolves");
    assert_eq!(plan.targets.len(), 1);
    assert_eq!(plan.targets[0].provider.id, provider_id);
    assert_eq!(plan.targets[0].upstream_model, upstream_model);
}

#[tokio::test]
async fn route_strategy_survives_reload_and_catalogue_reads_do_not_consume_rotation() {
    use gproxy_core::Host as _;
    use gproxy_store::records::{RouteInput, RouteMemberInput, RouteStrategy};
    let fixture = setup::fixture().await;
    let app = &fixture.app;
    let host = &app.inner.host;
    let control = &host.services.control;
    app.mutate(crate::ControlMutation::RouteMember(RouteMemberInput {
        route_id: fixture.route,
        provider_id: fixture.provider,
        upstream_model: "second-model".into(),
        tier: 0,
        weight: 100,
        enabled: true,
    }))
    .await
    .unwrap();
    let request = setup::request("route-strategy", "hi", &fixture.client_key);
    let identity = host.authenticate(&request).await.unwrap();
    for strategy in [
        RouteStrategy::RoundRobin,
        RouteStrategy::Weighted,
        RouteStrategy::Failover,
    ] {
        host.services
            .store
            .update_route(
                fixture.route,
                &RouteInput {
                    name: "route".into(),
                    strategy,
                    max_attempts: 2,
                    enabled: true,
                },
            )
            .await
            .unwrap();
        app.reload().await.unwrap();
        assert_eq!(control.current().routes[0].strategy, strategy);
        let picks = (0..6)
            .map(|_| {
                assert!(control.catalogue_visible(
                    &identity,
                    Some("public-model"),
                    &RoutingMode::Aggregated
                ));
                let plan = control
                    .resolve(Some("public-model"), &RoutingMode::Aggregated, None)
                    .unwrap();
                assert_eq!(plan.budget.max_attempts, 2);
                assert!(
                    plan.targets
                        .iter()
                        .all(|target| !target.rules.session_affinity)
                );
                plan.targets[0].upstream_model.clone()
            })
            .collect::<Vec<_>>();
        if strategy == RouteStrategy::Failover {
            assert!(picks.iter().all(|model| model == "upstream-model"));
        } else {
            assert_eq!(
                picks,
                [
                    "upstream-model",
                    "second-model",
                    "upstream-model",
                    "second-model",
                    "upstream-model",
                    "second-model"
                ]
            );
        }
    }
    app.shutdown();
}
