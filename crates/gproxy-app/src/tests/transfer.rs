use super::*;

#[tokio::test]
async fn credential_mutation_and_import_apply_default_labels() {
    let source_directory = tempfile::tempdir().unwrap();
    let destination_directory = tempfile::tempdir().unwrap();
    let source_key = [51; 32];
    let destination_key = [73; 32];
    let source = crate::App::start(test_config(
        source_directory.path(),
        crate::MasterKeyConfig::new(Some(source_key)),
    ))
    .await
    .unwrap();
    let provider = setup::id(
        source
            .mutate(crate::ControlMutation::Provider(
                gproxy_store::records::ProviderInput {
                    name: "export-provider".into(),
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
    let credential_id = setup::id(
        source
            .mutate(crate::ControlMutation::Credential {
                provider_id: provider,
                label: None,
                secret: json!({"api_key": "source-secret"}),
                enabled: true,
            })
            .await
            .unwrap(),
    );
    let source_credential = source
        .inner
        .host
        .services
        .store
        .credential(credential_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source_credential.label.as_deref(), Some("sourc…cret"));
    seed_admin_key(&source).await;
    let export = source
        .admin_dispatch(
            &admin_parts(http::Method::POST, "/admin/api/export"),
            Bytes::from_static(br#"{"include_secrets":true}"#),
        )
        .await
        .unwrap();
    assert_eq!(export.status(), http::StatusCode::OK);
    let mut export: gproxy_admin::dto::ConfigurationExportDto =
        serde_json::from_slice(export.body()).unwrap();
    export.data.credentials[0].config.label = None;

    let destination = crate::App::start(test_config(
        destination_directory.path(),
        crate::MasterKeyConfig::new(Some(destination_key)),
    ))
    .await
    .unwrap();
    seed_admin_key(&destination).await;
    let body = serde_json::to_vec(&gproxy_admin::dto::ConfigurationImportRequest {
        export,
        source_master_key: Some(base64::engine::general_purpose::STANDARD.encode(source_key)),
    })
    .unwrap();
    let imported = destination
        .admin_dispatch(
            &admin_parts(http::Method::POST, "/admin/api/import"),
            Bytes::from(body),
        )
        .await
        .unwrap();
    assert_eq!(imported.status(), http::StatusCode::OK);
    let credential = destination
        .inner
        .host
        .services
        .store
        .credential(1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(credential.label.as_deref(), Some("sourc…cret"));
    assert_eq!(
        destination
            .inner
            .host
            .services
            .cipher
            .open(&credential.envelope)
            .unwrap(),
        json!({"api_key": "source-secret"})
    );
    assert!(
        crate::secrets::EnvelopeCipher::new(Some(source_key))
            .open(&credential.envelope)
            .is_err()
    );
}

fn admin_parts(method: http::Method, path: &str) -> http::request::Parts {
    http::Request::builder()
        .method(method)
        .uri(path)
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {}", admin_key()),
        )
        .body(())
        .unwrap()
        .into_parts()
        .0
}

async fn seed_admin_key(app: &crate::AppHandle) {
    let store = &app.inner.host.services.store;
    let id = gproxy_admin::seed_first_admin(store, "transfer-admin", &setup::random_key())
        .await
        .unwrap()
        .unwrap();
    store
        .insert_user_key(&gproxy_store::records::UserKeyInput {
            user_id: id,
            digest: Sha256::digest(admin_key().as_bytes()).to_vec(),
            digest_version: crate::control::USER_KEY_DIGEST_VERSION,
            prefix: "transfer-adm".into(),
            envelope: app
                .inner
                .host
                .services
                .cipher
                .seal_user_key(&json!(admin_key()))
                .unwrap(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .unwrap();
}

fn admin_key() -> &'static str {
    static KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    KEY.get_or_init(setup::random_key)
}
