use bytes::Bytes;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use gproxy_channel_api::{AuthCodeStart, BoxFuture, DeviceInit, DevicePoll};
use gproxy_store::records::{CredentialEnvelope, ProviderInput};
use http::{Method, StatusCode};
use sha2::{Digest, Sha256};

use crate::dto::{ChannelDto, PortalModelDto};
use crate::{AdminError, PortalIdentity, State};

struct TestState {
    store: gproxy_store::Store,
    login_state: Mutex<HashMap<String, Vec<u8>>>,
    device_polls: Mutex<VecDeque<DevicePoll>>,
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

    fn login_state_get<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, AdminError>> {
        let value = self.login_state.lock().unwrap().get(key).cloned();
        Box::pin(async move { Ok(value) })
    }

    fn login_state_set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        _: Duration,
    ) -> BoxFuture<'a, Result<(), AdminError>> {
        self.login_state.lock().unwrap().insert(key.into(), value);
        Box::pin(async { Ok(()) })
    }

    fn login_state_delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), AdminError>> {
        self.login_state.lock().unwrap().remove(key);
        Box::pin(async { Ok(()) })
    }

    fn login_authcode_start<'a>(
        &'a self,
        _: &'a str,
        _: i64,
        _: &'a serde_json::Value,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, AdminError>> {
        Box::pin(async { Err(AdminError::BadRequest("unsupported".into())) })
    }

    fn login_authcode_exchange<'a>(
        &'a self,
        _: &'a str,
        _: i64,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: Option<&'a serde_json::Value>,
    ) -> BoxFuture<'a, Result<serde_json::Value, AdminError>> {
        Box::pin(async { Err(AdminError::BadRequest("unsupported".into())) })
    }

    fn login_device_start<'a>(
        &'a self,
        _: &'a str,
        provider_id: i64,
        _: &'a serde_json::Value,
    ) -> BoxFuture<'a, Result<DeviceInit, AdminError>> {
        Box::pin(async move {
            Ok(DeviceInit {
                device_code: provider_id.to_string(),
                user_code: provider_id.to_string(),
                verification_uri: "https://example.invalid".into(),
                interval_secs: 1,
            })
        })
    }

    fn login_device_poll<'a>(
        &'a self,
        _: &'a str,
        _: i64,
        _: &'a str,
    ) -> BoxFuture<'a, Result<DevicePoll, AdminError>> {
        let poll = self.device_polls.lock().unwrap().pop_front();
        Box::pin(async move {
            poll.ok_or_else(|| AdminError::Internal("missing test poll state".into()))
        })
    }

    fn login_cookie_exchange<'a>(
        &'a self,
        _: &'a str,
        _: i64,
        _: &'a str,
    ) -> BoxFuture<'a, Result<serde_json::Value, AdminError>> {
        Box::pin(async { Err(AdminError::BadRequest("unsupported".into())) })
    }

    fn channel_catalogue(&self) -> Vec<ChannelDto> {
        Vec::new()
    }

    fn portal_identity(&self, headers: &http::HeaderMap) -> Result<PortalIdentity, AdminError> {
        let authenticated = headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            == Some("Bearer portal-test-key");
        if !authenticated {
            return Err(AdminError::Unauthorized);
        }
        Ok(PortalIdentity {
            user_id: 11,
            user_key_id: 12,
            org_id: Some(13),
            team_id: Some(14),
            user_name: "portal-user".into(),
            key_prefix: Some("sk-gp-test".into()),
            key_label: None,
            expires_at: None,
        })
    }

    fn portal_models(&self, _: &PortalIdentity) -> Vec<PortalModelDto> {
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
async fn admin_and_portal_auth_boundaries_do_not_cross() {
    let state = state().await;
    let portal = key_parts(Method::GET, "/portal/api/context");
    let response = crate::portal_dispatch(&state, &portal, Bytes::new())
        .await
        .expect("portal context");
    assert_eq!(response.status(), StatusCode::OK);

    let admin_with_key = key_parts(Method::GET, "/admin/providers");
    let response = crate::dispatch(&state, &admin_with_key, Bytes::new())
        .await
        .expect("admin namespace");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let scoped = key_parts(Method::GET, "/portal/api/usage?from=0&to=1&user_key_id=999");
    let response = crate::portal_dispatch(&state, &scoped, Bytes::new())
        .await
        .expect("portal usage");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

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

    let portal_with_admin = parts(Method::GET, "/portal/api/context", Some(&cookie));
    let response = crate::portal_dispatch(&state, &portal_with_admin, Bytes::new())
        .await
        .expect("portal namespace");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let unknown = parts(Method::GET, "/admin/not-an-api", Some(&cookie));
    let response = crate::dispatch(&state, &unknown, Bytes::new())
        .await
        .expect("admin namespace is closed");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn pkce_challenge_matches_rfc_7636_vector() {
    assert_eq!(
        crate::handlers::login::state::pkce_challenge(
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        ),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[tokio::test]
async fn device_poll_keeps_pending_then_creates_ready_and_clears_denied() {
    let state = state().await;
    let provider_id = state
        .store
        .insert_provider(&ProviderInput {
            name: "device-flow".into(),
            label: None,
            channel: "codex".into(),
            settings: serde_json::json!({}),
            credential_strategy: "round_robin".into(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .expect("insert provider");
    state.device_polls.lock().unwrap().extend([
        DevicePoll::Pending,
        DevicePoll::Ready(serde_json::json!({})),
    ]);

    let session = start_device(&state, provider_id).await;
    let pending = poll_device(&state, &session).await;
    assert!(matches!(pending, crate::dto::DevicePollResponse::Pending));
    let ready = poll_device(&state, &session).await;
    assert!(matches!(
        ready,
        crate::dto::DevicePollResponse::Ready { .. }
    ));
    assert_eq!(state.store.admin_credentials().await.unwrap().len(), 1);

    state
        .device_polls
        .lock()
        .unwrap()
        .push_back(DevicePoll::Denied);
    let denied_session = start_device(&state, provider_id).await;
    let denied = poll_device(&state, &denied_session).await;
    assert!(matches!(denied, crate::dto::DevicePollResponse::Denied));
    let body = serde_json::to_vec(&crate::dto::DevicePollRequest {
        login_session_id: denied_session,
    })
    .unwrap();
    let error = crate::handlers::login::device_poll(&state, &Bytes::from(body))
        .await
        .expect_err("denied session must be cleared");
    assert!(matches!(error, AdminError::BadRequest(_)));
}

#[tokio::test]
async fn batch_reports_partial_failure_per_id() {
    let state = state().await;
    let id = state
        .store
        .insert_organization(&gproxy_store::records::OrganizationInput {
            name: "batch-org".into(),
            enabled: false,
        })
        .await
        .unwrap();
    seed_admin_key(&state).await;
    let body = serde_json::to_vec(&crate::dto::BatchRequest {
        action: crate::dto::BatchActionDto::Enable,
        ids: vec![id, i64::MAX],
    })
    .unwrap();
    let response = crate::dispatch(
        &state,
        &admin_parts(Method::POST, "/admin/batch/organizations"),
        Bytes::from(body),
    )
    .await
    .unwrap();
    let outcome: crate::dto::BatchResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(outcome.outcomes.len(), 2);
    assert!(outcome.outcomes[0].applied);
    assert!(!outcome.outcomes[1].applied);
    assert_eq!(outcome.outcomes[1].status, StatusCode::NOT_FOUND.as_u16());
    assert!(state.store.control_snapshot().await.unwrap().organizations[0].enabled);
}

#[tokio::test]
async fn batch_requires_admin_authorization_before_each_requested_mutation() {
    let state = state().await;
    let id = state
        .store
        .insert_organization(&gproxy_store::records::OrganizationInput {
            name: "protected-org".into(),
            enabled: false,
        })
        .await
        .unwrap();
    let body = serde_json::to_vec(&crate::dto::BatchRequest {
        action: crate::dto::BatchActionDto::Enable,
        ids: vec![id],
    })
    .unwrap();
    let response = crate::dispatch(
        &state,
        &parts(Method::POST, "/admin/batch/organizations", None),
        Bytes::from(body),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(!state.store.control_snapshot().await.unwrap().organizations[0].enabled);
}

async fn start_device(state: &TestState, provider_id: i64) -> String {
    let body = serde_json::to_vec(&crate::dto::DeviceStartRequest {
        provider_id,
        params: None,
        label: None,
    })
    .unwrap();
    let response = crate::handlers::login::device_start(state, &Bytes::from(body))
        .await
        .expect("device start");
    serde_json::from_slice::<crate::dto::DeviceStartResponse>(response.body())
        .unwrap()
        .login_session_id
}

async fn poll_device(state: &TestState, login_session_id: &str) -> crate::dto::DevicePollResponse {
    let body = serde_json::to_vec(&crate::dto::DevicePollRequest {
        login_session_id: login_session_id.into(),
    })
    .unwrap();
    let response = crate::handlers::login::device_poll(state, &Bytes::from(body))
        .await
        .expect("device poll");
    serde_json::from_slice(response.body()).unwrap()
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
        login_state: Mutex::new(HashMap::new()),
        device_polls: Mutex::new(VecDeque::new()),
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

fn key_parts(method: Method, uri: &str) -> http::request::Parts {
    http::Request::builder()
        .method(method)
        .uri(uri)
        .header(http::header::AUTHORIZATION, "Bearer portal-test-key")
        .body(())
        .expect("request")
        .into_parts()
        .0
}

fn admin_parts(method: Method, uri: &str) -> http::request::Parts {
    http::Request::builder()
        .method(method)
        .uri(uri)
        .header(http::header::AUTHORIZATION, "Bearer admin-test-key")
        .body(())
        .expect("request")
        .into_parts()
        .0
}

async fn seed_admin_key(state: &TestState) {
    let id = crate::seed_first_admin(&state.store, "batch-admin", "batch-password")
        .await
        .unwrap()
        .unwrap();
    state
        .store
        .create_admin_api_key(&Sha256::digest(b"admin-test-key"), id, 1)
        .await
        .unwrap();
}

fn envelope() -> CredentialEnvelope {
    CredentialEnvelope {
        ciphertext: vec![1],
        wrapped_key: vec![2],
        payload_nonce: vec![3],
        key_nonce: vec![4],
    }
}
