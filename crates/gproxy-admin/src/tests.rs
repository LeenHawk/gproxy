use bytes::Bytes;
use gproxy_channel_api::BoxFuture;
use gproxy_store::records::CredentialEnvelope;
use http::{Method, StatusCode};
use sha2::{Digest, Sha256};

use crate::dto::ChannelDto;
use crate::{AdminError, State};

struct TestState {
    store: gproxy_store::Store,
    _directory: tempfile::TempDir,
}

impl State for TestState {
    fn store(&self) -> &gproxy_store::Store {
        &self.store
    }

    fn seal_credential(&self, _: &serde_json::Value) -> Result<CredentialEnvelope, AdminError> {
        Ok(envelope())
    }

    fn seal_user_key(&self, _: &str) -> Result<CredentialEnvelope, AdminError> {
        Ok(envelope())
    }

    fn digest_user_key(&self, api_key: &str) -> (u32, Vec<u8>) {
        (1, Sha256::digest(api_key.as_bytes()).to_vec())
    }

    fn reveal_user_key(&self, _: i64, _: i64, _: i64) -> BoxFuture<'_, Result<String, AdminError>> {
        Box::pin(async { Ok("<redacted>".into()) })
    }

    fn admit_auth_attempt(
        &self,
        _: &'static str,
        _: &str,
    ) -> BoxFuture<'_, Result<(), AdminError>> {
        Box::pin(async { Ok(()) })
    }

    fn clear_auth_attempts(
        &self,
        _: &'static str,
        _: &str,
    ) -> BoxFuture<'_, Result<(), AdminError>> {
        Box::pin(async { Ok(()) })
    }

    fn reload(&self) -> BoxFuture<'_, Result<(), AdminError>> {
        Box::pin(async { Ok(()) })
    }

    fn channel_catalogue(&self) -> Vec<ChannelDto> {
        Vec::new()
    }

    fn normalize_provider_settings(
        &self,
        _: &str,
        settings: &serde_json::Value,
    ) -> Result<serde_json::Value, AdminError> {
        Ok(settings.clone())
    }
}

#[tokio::test]
async fn public_setup_and_protected_routes_share_one_auth_boundary() {
    let state = state().await;
    let session = parts(Method::GET, "/admin/session", None);
    let response = crate::dispatch(&state, &session, Bytes::new())
        .await
        .expect("session route");
    assert_eq!(response.status(), StatusCode::OK);

    let providers = parts(Method::GET, "/admin/providers", None);
    let response = crate::dispatch(&state, &providers, Bytes::new())
        .await
        .expect("provider route");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let setup = parts(Method::POST, "/admin/setup", None);
    let response = crate::dispatch(
        &state,
        &setup,
        Bytes::from_static(br#"{"username":"admin","password":"secret"}"#),
    )
    .await
    .expect("setup route");
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get(http::header::SET_COOKIE)
        .expect("session cookie")
        .to_str()
        .expect("cookie text")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_owned();

    let providers = parts(Method::GET, "/admin/providers", Some(&cookie));
    let response = crate::dispatch(&state, &providers, Bytes::new())
        .await
        .expect("provider route");
    assert_eq!(response.status(), StatusCode::OK);

    let unknown = parts(Method::GET, "/admin/not-an-api", Some(&cookie));
    let response = crate::dispatch(&state, &unknown, Bytes::new())
        .await
        .expect("admin namespace is closed");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn state() -> TestState {
    let directory = tempfile::tempdir().expect("admin tempdir");
    let store = gproxy_store::Store::open(gproxy_store::BackendConfig::Sqlite {
        path: directory.path().join("admin.db"),
    })
    .await
    .expect("admin store");
    TestState {
        store,
        _directory: directory,
    }
}

fn parts(method: Method, uri: &str, cookie: Option<&str>) -> http::request::Parts {
    let mut request = http::Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(http::header::COOKIE, cookie);
    }
    request.body(()).expect("request").into_parts().0
}

fn envelope() -> CredentialEnvelope {
    CredentialEnvelope {
        ciphertext: vec![1],
        wrapped_key: vec![2],
        payload_nonce: vec![3],
        key_nonce: vec![4],
    }
}
