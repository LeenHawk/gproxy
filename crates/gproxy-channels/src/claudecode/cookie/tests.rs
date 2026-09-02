use std::collections::VecDeque;
use std::future::Future;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use gproxy_channel_api::{BoxFuture, ClientProfilePreset, SimpleHttp};

use super::*;

#[test]
fn preserves_full_cookie_headers_and_normalizes_bare_keys() {
    assert_eq!(
        crate::shared::claude::cookie::normalize(
            "Cookie: cf_clearance=clear; sessionKey=sk-ant-sid01-example; __cf_bm=bm"
        )
        .as_deref(),
        Some("cf_clearance=clear; sessionKey=sk-ant-sid01-example; __cf_bm=bm")
    );
    assert_eq!(
        crate::shared::claude::cookie::normalize("sk-ant-sid02-example").as_deref(),
        Some("sessionKey=sk-ant-sid02-example")
    );
}

#[test]
fn cookie_only_refresh_retries_bootstrap_and_mints_oauth_secret() {
    let http = MockHttp::new([
        (
            http::StatusCode::FORBIDDEN,
            br#"<title>Just a moment...</title>"#.as_slice(),
        ),
        (
            http::StatusCode::OK,
            br#"{"usage":{}} {"account":{"memberships":[{"organization":{"uuid":"org-api","capabilities":["api"]}},{"organization":{"uuid":"org-sub","capabilities":["claude_max"]}}]}}"#,
        ),
        (
            http::StatusCode::OK,
            br#"{"redirect_uri":"https://platform.claude.com/oauth/code/callback?code=code-1&state=state"}"#,
        ),
        (
            http::StatusCode::OK,
            br#"{"access_token":"fresh","expires_in":3600,"scope":"user:inference user:file_upload"}"#,
        ),
        (
            http::StatusCode::OK,
            br#"{"account":{"uuid":"account-1","email":"user@example.com"},"organization":{"uuid":"org-sub","organization_type":"claude_max"}}"#,
        ),
    ]);
    let old = json!({
        "cookie": "cf_clearance=clear; sessionKey=sk-ant-sid01-example",
        "operator_note": "keep"
    });
    let refreshed = ready(super::super::auth::refresh(&old, &http)).unwrap();
    assert_eq!(refreshed["access_token"], "fresh");
    assert_eq!(refreshed["account_uuid"], "account-1");
    assert_eq!(refreshed["organization_uuid"], "org-sub");
    assert_eq!(refreshed["operator_note"], "keep");
    assert!(refreshed.get("refresh_token").is_none());
    assert!(refreshed["device_id"].as_str().is_some());

    let captured = http.captured.lock().unwrap();
    assert_eq!(captured.len(), 5);
    assert_eq!(captured[0].uri, "https://claude.ai/api/bootstrap");
    assert_eq!(captured[1].uri, "https://claude.ai/api/bootstrap");
    assert_eq!(
        captured[1].headers["cookie"],
        "cf_clearance=clear; sessionKey=sk-ant-sid01-example"
    );
    assert_eq!(
        captured[2].uri,
        "https://api.anthropic.com/v1/oauth/org-sub/authorize"
    );
    assert_eq!(captured[3].uri, auth::COOKIE_TOKEN_URL);
    assert!(
        std::str::from_utf8(&captured[2].body)
            .unwrap()
            .contains("user:inference")
    );
    assert!(
        std::str::from_utf8(&captured[3].body)
            .unwrap()
            .contains("grant_type=authorization_code")
    );
    assert!(captured[..4].iter().all(|request| {
        request.preset == Some(ClientProfilePreset::Chrome148) && request.required
    }));
    assert_eq!(captured[4].preset, None);
    assert!(!captured[4].required);
}

struct Captured {
    uri: String,
    headers: http::HeaderMap,
    body: Bytes,
    preset: Option<ClientProfilePreset>,
    required: bool,
}

struct MockHttp {
    responses: Mutex<VecDeque<(http::StatusCode, Bytes)>>,
    captured: Mutex<Vec<Captured>>,
}

impl MockHttp {
    fn new<const N: usize>(responses: [(http::StatusCode, &'static [u8]); N]) -> Self {
        Self {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(status, body)| (status, Bytes::from_static(body)))
                    .collect(),
            ),
            captured: Mutex::new(Vec::new()),
        }
    }
}

impl SimpleHttp for MockHttp {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
        let preset = request
            .extensions()
            .get::<ClientProfile>()
            .and_then(|profile| profile.preset);
        let required = request
            .extensions()
            .get::<RequiredClientProfile>()
            .is_some();
        let (parts, body) = request.into_parts();
        self.captured.lock().unwrap().push(Captured {
            uri: parts.uri.to_string(),
            headers: parts.headers,
            body,
            preset,
            required,
        });
        let (status, body) = self.responses.lock().unwrap().pop_front().unwrap();
        Box::pin(async move {
            http::Response::builder()
                .status(status)
                .body(body)
                .map_err(|error| ChannelError::Login(error.to_string()))
        })
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}
