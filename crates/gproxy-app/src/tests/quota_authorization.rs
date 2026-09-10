use super::setup;
use crate::{AppHandle, ControlMutation, MutationResult};
use bytes::Bytes;
use gproxy_admin::State;
use gproxy_core::{CredentialId, CredentialStore};
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn separate_query_secret_patch_preserves_primary_health_and_rotation() {
    let fixture = setup::fixture().await;
    let app = &fixture.app;
    let admin_key = setup::random_key();
    let user =
        gproxy_admin::seed_first_admin(app.store(), "query-auth-admin", &setup::random_key())
            .await
            .unwrap()
            .unwrap();
    app.store()
        .insert_user_key(&gproxy_store::records::UserKeyInput {
            user_id: user,
            digest: Sha256::digest(admin_key.as_bytes()).to_vec(),
            digest_version: crate::control::USER_KEY_DIGEST_VERSION,
            prefix: "query-auth".into(),
            envelope: app
                .inner
                .host
                .services
                .cipher
                .seal_user_key(&json!(admin_key))
                .unwrap(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .unwrap();
    let provider_id = provider(app, "openrouter").await;
    let primary = setup::random_key();
    let management = setup::random_key();
    let MutationResult::Id(id) = app
        .mutate(ControlMutation::Credential {
            provider_id,
            label: None,
            secret: json!({"api_key":primary}),
            enabled: true,
        })
        .await
        .unwrap()
    else {
        panic!("credential id")
    };
    let version = app.store().credential(id).await.unwrap().unwrap().version;
    app.store()
        .record_credential_health(&gproxy_store::records::CredentialHealthInput {
            credential_id: id,
            model: "model".into(),
            credential_version: version,
            version: 1,
            state: gproxy_store::records::CredentialHealthState::Dead,
            observed_at: crate::quota_refresh::now(),
            response_status: Some(401),
            detail: None,
        })
        .await
        .unwrap();
    let mut request = json!({"provider_id":provider_id,"label":null,"kind":"api_key","secret":null,
        "quota_secret":{"quota_api_key":management},"enabled":true,"weight":100,"rpm_limit":null,
        "tpm_limit":null,"proxy_url":null,"tls_fingerprint":null});
    assert_eq!(save(app, id, &admin_key, &request).await, StatusCode::OK);
    let secret = app.reveal_credential_secret(id).await.unwrap();
    assert_eq!(secret["api_key"], primary);
    assert_eq!(secret["quota_api_key"], management);
    assert_eq!(secret["quota_channel"], "openrouter");
    let health = app
        .store()
        .credential_health()
        .await
        .unwrap()
        .into_iter()
        .find(|value| value.credential_id == id)
        .unwrap();
    assert_eq!(
        health.state,
        gproxy_store::records::CredentialHealthState::Dead
    );
    let rotated_version = app.store().credential(id).await.unwrap().unwrap().version;
    assert_eq!(health.credential_version, rotated_version);
    app.inner
        .host
        .persist_rotation(
            CredentialId(id),
            json!({"api_key":primary,"access_token":setup::random_key()}),
            rotated_version,
        )
        .await
        .unwrap();
    let secret = app.reveal_credential_secret(id).await.unwrap();
    assert_eq!(secret["quota_api_key"], management);
    assert!(app.credential_quota_snapshot(id).await.is_ok());
    request["quota_secret"] = json!({"quota_api_key":null});
    assert_eq!(save(app, id, &admin_key, &request).await, StatusCode::OK);
    let secret = app.reveal_credential_secret(id).await.unwrap();
    assert_eq!(secret["api_key"], primary);
    assert!(secret.get("quota_api_key").is_none());
    request["quota_secret"] = json!({"api_key":management});
    assert_eq!(
        save(app, id, &admin_key, &request).await,
        StatusCode::BAD_REQUEST
    );
    request["quota_secret"] = json!({"quota_api_key":management});
    assert_eq!(save(app, id, &admin_key, &request).await, StatusCode::OK);
    let target = provider(app, "xai").await;
    request["provider_id"] = json!(target);
    request["quota_secret"] = Value::Null;
    assert_eq!(save(app, id, &admin_key, &request).await, StatusCode::OK);
    let secret = app.reveal_credential_secret(id).await.unwrap();
    assert_eq!(secret["api_key"], primary);
    assert!(
        !secret
            .as_object()
            .unwrap()
            .keys()
            .any(|key| key.starts_with("quota_"))
    );
    request["secret"] = json!({"api_key":primary,"quota_api_key":management});
    assert_eq!(
        save(app, id, &admin_key, &request).await,
        StatusCode::BAD_REQUEST
    );
}

async fn provider(app: &AppHandle, channel: &str) -> i64 {
    let MutationResult::Id(id) = app
        .mutate(ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: channel.into(),
                label: None,
                channel: channel.into(),
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
        panic!("provider id")
    };
    id
}

async fn save(app: &AppHandle, id: i64, key: &str, value: &Value) -> StatusCode {
    let parts = http::Request::builder()
        .method(Method::PATCH)
        .uri(format!("/admin/api/credentials/{id}"))
        .header(http::header::AUTHORIZATION, format!("Bearer {key}"))
        .body(())
        .unwrap()
        .into_parts()
        .0;
    app.admin_dispatch(&parts, Bytes::from(value.to_string()))
        .await
        .unwrap()
        .status()
}
