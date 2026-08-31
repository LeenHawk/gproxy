use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use gproxy_store::records::CredentialEnvelope;
use http::Method;
use sha2::{Digest, Sha256};

use super::TestState;

pub(super) async fn state() -> TestState {
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

pub(super) fn parts(method: Method, uri: &str, cookie: Option<&str>) -> http::request::Parts {
    let mut request = http::Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(http::header::COOKIE, cookie);
    }
    request.body(()).expect("request").into_parts().0
}

pub(super) fn key_parts(method: Method, uri: &str) -> http::request::Parts {
    http::Request::builder()
        .method(method)
        .uri(uri)
        .header(http::header::AUTHORIZATION, "Bearer portal-test-key")
        .body(())
        .expect("request")
        .into_parts()
        .0
}

pub(super) fn admin_parts(method: Method, uri: &str) -> http::request::Parts {
    http::Request::builder()
        .method(method)
        .uri(uri)
        .header(http::header::AUTHORIZATION, "Bearer admin-test-key")
        .body(())
        .expect("request")
        .into_parts()
        .0
}

pub(super) async fn seed_admin_key(state: &TestState) {
    let id = crate::seed_first_admin(&state.store, "batch-admin", "batch-password")
        .await
        .unwrap()
        .unwrap();
    state
        .store
        .insert_user_key(&gproxy_store::records::UserKeyInput {
            user_id: id,
            digest: Sha256::digest(b"admin-test-key").to_vec(),
            digest_version: 1,
            prefix: "admin-test-k".into(),
            envelope: envelope(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .unwrap();
}

pub(super) fn envelope() -> CredentialEnvelope {
    CredentialEnvelope {
        ciphertext: vec![1],
        wrapped_key: vec![2],
        payload_nonce: vec![3],
        key_nonce: vec![4],
    }
}
