use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use http::{Request, Response, header};
use serde_json::{Value, json};

use super::{login, login_social};
use crate::http::client::{ClientError, UpstreamClient};

struct MockClient {
    requests: Mutex<Vec<Request<Bytes>>>,
    responses: Mutex<VecDeque<Response<Bytes>>>,
}

#[async_trait]
impl UpstreamClient for MockClient {
    async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ClientError::Transport("missing response".into()))
    }
}

#[tokio::test]
async fn email_pin_login_selects_personal_workspace() {
    let client = Arc::new(MockClient {
        requests: Mutex::new(Vec::new()),
        responses: Mutex::new(VecDeque::from([
            Response::new(Bytes::from_static(br#"{"expiresAt":999999}"#)),
            Response::new(Bytes::from_static(
                br#"{"type":"success","userId":"u_test","sessionToken":"st_test"}"#,
            )),
            Response::new(Bytes::from_static(
                br#"{"userId":"u_test","organizations":[{"organizationId":"org_test","workspaces":[{"workspaceId":"ws_shared","type":"shared"},{"workspaceId":"ws_personal","type":"personal"}]}]}"#,
            )),
        ])),
    });
    let client_dyn: Arc<dyn UpstreamClient> = client.clone();
    let started = login::start(
        &client_dyn,
        &json!({"email":"user@example.com"}),
        "state-email",
    )
    .await
    .unwrap();
    let secret = login::exchange(&client_dyn, "123456", started.extra.as_ref())
        .await
        .unwrap();

    assert_eq!(secret["session_token"], "st_test");
    assert_eq!(secret["workspace_id"], "ws_personal");
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests[0].uri().path(), "/api/auth/magic-link/request");
    assert_eq!(requests[1].uri().path(), "/api/signIn");
    assert_eq!(
        requests[2].headers()[header::AUTHORIZATION],
        "Bearer st_test"
    );
}

#[tokio::test]
async fn social_login_validates_callback_and_exchanges_code() {
    let client = Arc::new(MockClient {
        requests: Mutex::new(Vec::new()),
        responses: Mutex::new(VecDeque::from([
            Response::new(Bytes::from_static(
                br#"{"type":"success","userId":"u_social","sessionToken":"st_social"}"#,
            )),
            Response::new(Bytes::from_static(
                br#"{"userId":"u_social","organizations":[{"organizationId":"org_social","workspaces":[{"workspaceId":"ws_social","type":"personal"}]}]}"#,
            )),
        ])),
    });
    let client_dyn: Arc<dyn UpstreamClient> = client.clone();
    let google = login::start(
        &client_dyn,
        &json!({"auth_method":"google"}),
        "state-social",
    )
    .await
    .unwrap();
    let state = google.extra.as_ref().unwrap()["callback_state"]
        .as_str()
        .unwrap();
    assert!(
        google
            .authorize_url
            .starts_with("https://accounts.google.com/")
    );
    let callback = format!(
        "{}?code=provider-code&state={}",
        login_social::CALLBACK_URL,
        percent_encode(state)
    );
    let secret = login::exchange(&client_dyn, &callback, google.extra.as_ref())
        .await
        .unwrap();

    assert_eq!(secret["session_token"], "st_social");
    let requests = client.requests.lock().unwrap();
    let body: Value = serde_json::from_slice(requests[0].body()).unwrap();
    assert_eq!(body["type"], "oauth2code");
    assert_eq!(body["provider"], "google");
    assert_eq!(body["code"], "provider-code");
    assert!(login_social::callback_code(&callback, "wrong-state").is_err());

    let (microsoft, _) = login_social::authorize_url("microsoft", "state-ms");
    assert!(microsoft.starts_with("https://login.microsoftonline.com/common/"));
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
