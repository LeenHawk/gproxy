mod admin;
mod fingerprint;
mod pressure;
mod quota;
mod setup;
mod tokenizer;

use base64::Engine as _;
use bytes::Bytes;
use gproxy_core::CaptureSink;
use serde_json::json;
use sha2::{Digest as _, Sha256};

fn generation_operation() -> gproxy_protocol::OperationKey {
    gproxy_protocol::OperationKey::content(
        gproxy_protocol::Operation::GenerateContent,
        gproxy_protocol::ContentGenerationKind::OpenAiChat,
    )
}

#[tokio::test]
async fn log_sink_redacts_by_default_and_writes_clear_only_when_disabled() {
    let setup::Fixture {
        app,
        provider,
        credential,
        client_key,
        ..
    } = setup::fixture().await;
    for (key, value) in [
        (crate::cleanup::RETENTION_DAYS, json!(1)),
        (crate::logging::ENABLE_DOWNSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_DOWNSTREAM_LOG_BODY, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG_BODY, json!(true)),
    ] {
        setting(&app, key, value).await;
    }
    let mut request = setup::request("redacted", "hi", &client_key);
    request
        .headers
        .insert("x-api-key", "fixture-value".parse().unwrap());
    request.query = Some("token=fixture-value&mode=test".into());
    request.body = Bytes::from_static(br#"{"token":"fixture-value","ok":true}"#);
    crate::logging::begin(&app.inner.host, &request)
        .await
        .expect("downstream capture");
    CaptureSink::record(&app.inner.host, &capture(&request, provider, credential)).await;
    let detail = load_detail(&app, &request.request_id).await;
    let headers = detail.downstream.input.request_headers.as_ref().unwrap();
    assert_eq!(headers["authorization"], "[redacted]");
    assert_eq!(headers["x-api-key"], "[redacted]");
    assert_eq!(
        detail.downstream.input.query.as_deref(),
        Some("token=[redacted]&mode=test")
    );
    assert!(!body(&detail).contains("fixture-value"));

    setting(&app, crate::logging::DISABLE_LOG_REDACTION, json!(true)).await;
    let mut clear = request.clone();
    clear.request_id = "request-clear".into();
    crate::logging::begin(&app.inner.host, &clear)
        .await
        .expect("clear capture");
    CaptureSink::record(&app.inner.host, &capture(&clear, provider, credential)).await;
    let detail = load_detail(&app, &clear.request_id).await;
    let headers = detail.downstream.input.request_headers.as_ref().unwrap();
    assert_eq!(headers["authorization"], format!("Bearer {client_key}"));
    assert_eq!(headers["x-api-key"], "fixture-value");
    assert!(body(&detail).contains("fixture-value"));
}

#[tokio::test]
async fn upstream_metadata_is_recorded_without_bodies() {
    let setup::Fixture {
        app,
        provider,
        credential,
        client_key,
        ..
    } = setup::fixture().await;
    for (key, value) in [
        (crate::cleanup::RETENTION_DAYS, json!(1)),
        (crate::logging::ENABLE_DOWNSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG_BODY, json!(false)),
    ] {
        setting(&app, key, value).await;
    }
    let request = setup::request("metadata", "hi", &client_key);
    crate::logging::begin(&app.inner.host, &request)
        .await
        .expect("downstream capture");
    CaptureSink::record(&app.inner.host, &capture(&request, provider, credential)).await;
    let detail = load_detail(&app, &request.request_id).await;
    let upstream = &detail.upstream[0].input;
    assert_eq!(upstream.request_method.as_deref(), Some("POST"));
    assert!(upstream.request_headers.is_some());
    assert_eq!(upstream.response_status, Some(200));
    assert!(upstream.response_headers.is_some());
    assert!(upstream.request_body.is_none());
    assert!(upstream.response_body.is_none());
}

fn capture(
    request: &gproxy_core::RequestCtx,
    provider: i64,
    credential: i64,
) -> gproxy_core::host::Capture {
    let mut response_headers = http::HeaderMap::new();
    response_headers.insert("set-cookie", "fixture-value".parse().unwrap());
    gproxy_core::host::Capture {
        request_id: request.request_id.clone(),
        provider_id: Some(provider),
        credential_id: Some(gproxy_core::CredentialId(credential)),
        upstream_url: Some("https://example.invalid/v1/test?token=fixture-value".into()),
        request_method: Some(http::Method::POST),
        request_headers: Some(request.headers.clone()),
        request_body: request.body.clone(),
        response_status: Some(http::StatusCode::OK),
        response_headers: Some(response_headers),
        response_body: Some(Bytes::from_static(br#"{"token":"fixture-value"}"#)),
    }
}

async fn setting(app: &crate::AppHandle, key: &str, value: serde_json::Value) {
    app.mutate(crate::ControlMutation::Setting(
        gproxy_store::records::SettingInput {
            key: key.into(),
            value,
        },
    ))
    .await
    .expect("log setting");
}

async fn load_detail(app: &crate::AppHandle, request_id: &str) -> gproxy_store::records::LogDetail {
    app.inner
        .host
        .services
        .store
        .log_detail(request_id)
        .await
        .unwrap()
        .expect("log detail")
}

fn body(detail: &gproxy_store::records::LogDetail) -> String {
    String::from_utf8(detail.upstream[0].input.request_body.clone().unwrap()).unwrap()
}

#[tokio::test]
async fn key_rotation_reseals_every_secret_and_updates_fingerprint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_path_buf();
    let app = crate::App::start(test_config(&path, crate::MasterKeyConfig::new(None)))
        .await
        .unwrap();
    let provider = setup::id(
        app.mutate(crate::ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "rotation-provider".into(),
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
    for value in ["first", "second"] {
        app.mutate(crate::ControlMutation::Credential {
            provider_id: provider,
            label: Some(value.into()),
            secret: json!({"api_key": value}),
            enabled: true,
        })
        .await
        .unwrap();
    }
    let user = setup::id(
        app.mutate(crate::ControlMutation::User(
            gproxy_store::records::UserInput {
                name: "rotation-user".into(),
                organization_id: None,
                team_id: None,
                password_hash: None,
                enabled: true,
                is_admin: false,
            },
        ))
        .await
        .unwrap(),
    );
    for value in ["user-key-first", "user-key-second"] {
        app.mutate(crate::ControlMutation::UserKey {
            user_id: user,
            api_key: value.into(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .unwrap();
    }

    let first_key = [17; 32];
    let second_key = [29; 32];
    let app = crate::App::start(test_config(
        &path,
        crate::MasterKeyConfig::new(None).rotate_to_key(first_key),
    ))
    .await
    .unwrap();
    assert_secret_inventory(&app, Some(&first_key)).await;
    let app = crate::App::start(test_config(
        &path,
        crate::MasterKeyConfig::new(Some(first_key)).rotate_to_key(second_key),
    ))
    .await
    .unwrap();
    assert_secret_inventory(&app, Some(&second_key)).await;
    let app = crate::App::start(test_config(
        &path,
        crate::MasterKeyConfig::new(Some(second_key)).rotate_to_plaintext(),
    ))
    .await
    .unwrap();
    assert_secret_inventory(&app, None).await;
}

#[tokio::test]
async fn sealed_store_without_key_names_required_fingerprint() {
    let directory = tempfile::tempdir().unwrap();
    let key = [41; 32];
    let app = crate::App::start(test_config(
        directory.path(),
        crate::MasterKeyConfig::new(Some(key)),
    ))
    .await
    .unwrap();
    let provider = setup::id(
        app.mutate(crate::ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "sealed-provider".into(),
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
        provider_id: provider,
        label: None,
        secret: json!({"api_key": "sealed-fixture"}),
        enabled: true,
    })
    .await
    .unwrap();
    drop(app);
    let error = match crate::App::start(test_config(
        directory.path(),
        crate::MasterKeyConfig::new(None),
    ))
    .await
    {
        Ok(_) => panic!("sealed store started without its key"),
        Err(error) => error,
    };
    let required = crate::key_rotation::fingerprint(Some(&key)).unwrap();
    assert!(error.to_string().contains(&required));
}

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

fn test_config(path: &std::path::Path, keys: crate::MasterKeyConfig) -> crate::Config {
    crate::Config::sqlite("127.0.0.1:0".parse().unwrap(), path.to_path_buf(), keys)
}

fn admin_parts(method: http::Method, path: &str) -> http::request::Parts {
    http::Request::builder()
        .method(method)
        .uri(path)
        .header(http::header::AUTHORIZATION, "Bearer transfer-admin-key")
        .body(())
        .unwrap()
        .into_parts()
        .0
}

async fn seed_admin_key(app: &crate::AppHandle) {
    let store = &app.inner.host.services.store;
    let id = gproxy_admin::seed_first_admin(store, "transfer-admin", "transfer-password")
        .await
        .unwrap()
        .unwrap();
    store
        .insert_user_key(&gproxy_store::records::UserKeyInput {
            user_id: id,
            digest: Sha256::digest(b"transfer-admin-key").to_vec(),
            digest_version: crate::control::USER_KEY_DIGEST_VERSION,
            prefix: "transfer-adm".into(),
            envelope: app
                .inner
                .host
                .services
                .cipher
                .seal_user_key(&json!("transfer-admin-key"))
                .unwrap(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .unwrap();
}

async fn assert_secret_inventory(app: &crate::AppHandle, key: Option<&[u8; 32]>) {
    let services = &app.inner.host.services;
    let inventory = services.store.secret_inventory().await.unwrap();
    assert_eq!(inventory.credentials.len(), 2);
    assert_eq!(inventory.user_keys.len(), 2);
    let expected = crate::key_rotation::fingerprint(key);
    match inventory.fingerprint {
        gproxy_store::records::MasterKeyFingerprint::Plaintext => assert!(expected.is_none()),
        gproxy_store::records::MasterKeyFingerprint::Sealed(value) => {
            assert_eq!(Some(value), expected)
        }
        gproxy_store::records::MasterKeyFingerprint::Missing => panic!("fingerprint missing"),
    }
    for secret in &inventory.credentials {
        services.cipher.open(&secret.envelope).unwrap();
    }
    for secret in &inventory.user_keys {
        services.cipher.open_user_key(&secret.envelope).unwrap();
    }
}
