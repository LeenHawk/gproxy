use gproxy_core::{ControlPlane, RoutingMode};
use serde_json::json;

#[tokio::test]
async fn provider_creation_seeds_empty_rules_and_scoped_variants_resolve() {
    let directory = tempfile::tempdir().unwrap();
    let app = crate::App::start(super::test_config(
        directory.path(),
        crate::MasterKeyConfig::new(None),
    ))
    .await
    .unwrap();
    let crate::MutationResult::Id(provider_id) = app
        .mutate(crate::ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "scoped".into(),
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
        .unwrap()
    else {
        panic!("provider mutation returned no id");
    };
    app.inner
        .host
        .services
        .store
        .insert_provider_model(&gproxy_store::records::ProviderModelInput {
            provider_id,
            model_id: "gpt-base".into(),
            display_name: None,
            variants: Some(json!(["gpt-base-thinking-high", "gpt-base-image-generate"])),
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
    app.reload().await.unwrap();

    let stored = app.inner.host.services.control.current();
    let marker = format!("gproxy:provider-default:{provider_id}");
    let rule_set = stored
        .rule_sets
        .iter()
        .find(|set| set.description.as_deref() == Some(marker.as_str()))
        .unwrap();
    assert!(
        stored
            .rules
            .iter()
            .all(|rule| rule.rule_set_id != rule_set.id)
    );
    let mode = RoutingMode::Scoped {
        provider: "scoped".into(),
    };
    assert_eq!(
        app.inner
            .host
            .services
            .control
            .resolve_variant("gpt-base-image-generate", &mode),
        Some("gpt-base".into())
    );
}

#[tokio::test]
async fn provider_variants_are_advertised_in_aggregated_and_scoped_model_lists() {
    let fixture = super::setup::fixture().await;
    let app = &fixture.app;
    let store = &app.inner.host.services.store;
    store
        .update_provider(
            fixture.provider,
            &gproxy_store::records::ProviderInput {
                name: "provider".into(),
                label: None,
                channel: "openai".into(),
                settings: json!({"auto_refresh_models": false}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: true,
            },
        )
        .await
        .unwrap();
    for (model_id, variants, enabled) in [
        ("visible", json!(["visible", "visible-high"]), true),
        (
            "hidden",
            json!({"expose_base": false, "variants": ["hidden", "custom-name"]}),
            true,
        ),
        ("disabled", json!(["disabled-high"]), false),
    ] {
        store
            .insert_provider_model(&gproxy_store::records::ProviderModelInput {
                provider_id: fixture.provider,
                model_id: model_id.into(),
                display_name: Some("Model display name".into()),
                variants: Some(variants),
                context_window: Some(128_000),
                max_output_tokens: Some(8_192),
                thinking_supported: Some(true),
                thinking_adaptive_supported: None,
                thinking_enabled_supported: None,
                metadata: Default::default(),
                enabled,
            })
            .await
            .unwrap();
    }
    app.reload().await.unwrap();
    let control = &app.inner.host.services.control;
    let catalogue = control.provider_catalogue();
    assert_eq!(catalogue.len(), 3);
    for model in &catalogue {
        assert_eq!(model.display_name.as_deref(), Some("Model display name"));
        assert_eq!(model.context_window, Some(128_000));
        assert_eq!(model.max_output_tokens, Some(8_192));
        assert_eq!(model.thinking_supported, Some(true));
    }
    for (mode, prefix) in [
        (RoutingMode::Aggregated, "provider/"),
        (
            RoutingMode::Scoped {
                provider: "provider".into(),
            },
            "",
        ),
    ] {
        let mut request = super::setup::request("variant-list", "", &fixture.client_key);
        request.method = http::Method::GET;
        request.path = "/v1/models".into();
        request.body = bytes::Bytes::new();
        request.mode = mode.clone();
        let response = app.execute(request).await.unwrap();
        assert_eq!(response.status, http::StatusCode::OK);
        let gproxy_core::ResponseBody::Full(body) = response.body else {
            panic!("model list must be buffered");
        };
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let ids = value["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        let mut expected = vec![
            format!("{prefix}custom-name"),
            format!("{prefix}visible"),
            format!("{prefix}visible-high"),
        ];
        if !prefix.is_empty() {
            expected.push("public-model".into());
        }
        expected.sort();
        assert_eq!(ids, expected);
        assert_eq!(
            control.resolve_variant(&format!("{prefix}custom-name"), &mode),
            Some(format!("{prefix}hidden")),
        );
    }
}
