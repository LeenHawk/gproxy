use gproxy_core::{ConfiguredFingerprint, ControlPlane, RoutingMode};
use serde_json::json;

#[tokio::test]
async fn explicit_fingerprints_need_no_instance_gate_and_credential_wins() {
    let super::setup::Fixture {
        app,
        provider,
        credential,
        ..
    } = super::setup::fixture().await;
    let store = &app.inner.host.services.store;
    store
        .update_provider(
            provider,
            &gproxy_store::records::ProviderInput {
                name: "provider".into(),
                label: None,
                channel: "openai".into(),
                settings: json!({}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: Some(json!({"headers": {"x-fingerprint-owner": "provider"}})),
                enabled: true,
            },
        )
        .await
        .unwrap();
    app.reload().await.unwrap();
    assert_eq!(fingerprint_owner(&app), "provider");

    let stored = store
        .admin_credentials()
        .await
        .unwrap()
        .into_iter()
        .find(|value| value.id == credential)
        .unwrap();
    store
        .update_credential(
            credential,
            &gproxy_store::records::CredentialUpdateInput {
                provider_id: provider,
                label: stored.label,
                kind: stored.kind,
                envelope: None,
                enabled: stored.enabled,
                weight: stored.weight,
                rpm_limit: stored.rpm_limit,
                tpm_limit: stored.tpm_limit,
                proxy_url: stored.proxy_url,
                tls_fingerprint: Some(json!({"headers": {"x-fingerprint-owner": "credential"}})),
            },
        )
        .await
        .unwrap();
    app.reload().await.unwrap();
    assert_eq!(fingerprint_owner(&app), "credential");
}

fn fingerprint_owner(app: &crate::AppHandle) -> String {
    let plan = app
        .inner
        .host
        .services
        .control
        .resolve(Some("public-model"), &RoutingMode::Aggregated, None)
        .unwrap();
    let Some(ConfiguredFingerprint::Usable(fingerprint)) = &plan.targets[0].provider.fingerprint
    else {
        panic!("explicit fingerprint was not compiled")
    };
    fingerprint.headers["x-fingerprint-owner"]
        .to_str()
        .unwrap()
        .to_owned()
}
