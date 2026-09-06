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
