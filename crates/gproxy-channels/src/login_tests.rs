use std::future::Future;
use std::task::{Context, Poll, Waker};

use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStartCtx, Channel, CredentialAcquisition, CredentialKind,
    DevicePoll, DevicePollCtx, DeviceStartCtx,
};
use serde_json::{Value, json};

use crate::shared::login::test::MockHttp;

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn start(channel: &dyn Channel, http: &MockHttp, settings: &Value, params: &Value) -> String {
    let login = channel.login().expect("interactive login");
    run(login.adapter.device_start(
        http,
        DeviceStartCtx {
            provider_settings: settings,
            params,
        },
    ))
    .expect("device start")
    .device_code
}

fn poll(channel: &dyn Channel, http: &MockHttp, settings: &Value, device_code: &str) -> DevicePoll {
    let login = channel.login().expect("interactive login");
    run(login.adapter.device_poll(
        http,
        DevicePollCtx {
            provider_settings: settings,
            device_code,
        },
    ))
    .expect("device poll")
}

fn ready(value: DevicePoll, kind: CredentialKind) -> CredentialAcquisition {
    let DevicePoll::Ready(acquired) = value else {
        panic!("expected ready")
    };
    assert_eq!(acquired.kind, kind);
    acquired
}

fn refresh(channel: &dyn Channel, http: &MockHttp, secret: &Value, settings: &Value) -> Value {
    run(channel
        .refresh(secret, settings, http)
        .expect("refresh supported"))
    .expect("first refresh")
}

#[test]
fn copilot_device_login_and_first_refresh() {
    let channel = crate::CopilotCliChannel;
    let http = MockHttp::new(&[
        (
            200,
            r#"{"device_code":"dc","user_code":"ABCD","verification_uri":"https://github.com/login/device","interval":1}"#,
        ),
        (200, r#"{"error":"authorization_pending"}"#),
        (200, r#"{"error":"access_denied"}"#),
        (200, r#"{"access_token":"github-token"}"#),
        (200, r#"{"token":"copilot-token","expires_at":2000000000}"#),
    ]);
    let code = start(&channel, &http, &json!({}), &json!({}));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Pending
    ));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Denied
    ));
    let acquired = ready(
        poll(&channel, &http, &json!({}), &code),
        CredentialKind::ApiKey,
    );
    assert_eq!(acquired.secret["github_token"], "github-token");
    assert_eq!(
        refresh(&channel, &http, &acquired.secret, &json!({}))["copilot_token"],
        "copilot-token"
    );
}

#[test]
fn cline_device_login_registers_api_key_and_refreshes() {
    let channel = crate::ClineChannel;
    let http = MockHttp::new(&[
        (
            200,
            r#"{"device_code":"dc","user_code":"CLINE","verification_uri":"https://workos.test","interval":1}"#,
        ),
        (400, r#"{"error":"authorization_pending"}"#),
        (400, r#"{"error":"access_denied"}"#),
        (
            200,
            r#"{"access_token":"workos-a","refresh_token":"workos-r"}"#,
        ),
        (
            200,
            r#"{"success":true,"data":{"accessToken":"cline-a","refreshToken":"cline-r","userInfo":{"clineUserId":"user-1","email":"u@example.test"}}}"#,
        ),
        (
            200,
            r#"{"success":true,"data":{"accessToken":"cline-b","refreshToken":"cline-r2"}}"#,
        ),
    ]);
    let code = start(
        &channel,
        &http,
        &json!({"base_url":"https://cline.test/api/v1"}),
        &json!({}),
    );
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Pending
    ));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Denied
    ));
    let settings = json!({"base_url":"https://cline.test/api/v1"});
    let acquired = ready(
        poll(&channel, &http, &settings, &code),
        CredentialKind::ApiKey,
    );
    assert_eq!(acquired.secret["user_id"], "user-1");
    assert_eq!(
        refresh(&channel, &http, &acquired.secret, &settings)["api_key"],
        "cline-b"
    );
}

#[test]
fn workbuddy_device_login_fetches_account_and_refreshes() {
    let channel = crate::WorkBuddyChannel;
    let settings = json!({"base_url":"https://workbuddy.test"});
    let http = MockHttp::new(&[
        (
            200,
            r#"{"code":0,"data":{"state":"state-1","authUrl":"https://login.test"}}"#,
        ),
        (200, r#"{"code":11217,"msg":"pending"}"#),
        (
            200,
            r#"{"code":0,"data":{"accessToken":"wb-a","refreshToken":"wb-r","expiresIn":3600}}"#,
        ),
        (200, r#"{"code":12151,"msg":"pending"}"#),
        (
            200,
            r#"{"code":0,"data":{"accessToken":"wb-a","refreshToken":"wb-r","expiresIn":3600}}"#,
        ),
        (
            200,
            r#"{"code":0,"data":{"uid":"user-2","enterpriseId":"ent-1","departmentFullName":"R&D"}}"#,
        ),
        (
            200,
            r#"{"code":0,"data":{"accessToken":"wb-b","refreshToken":"wb-r2","expiresIn":3600}}"#,
        ),
    ]);
    let code = start(&channel, &http, &settings, &json!({}));
    assert!(matches!(
        poll(&channel, &http, &settings, &code),
        DevicePoll::Pending
    ));
    assert!(matches!(
        poll(&channel, &http, &settings, &code),
        DevicePoll::Pending
    ));
    let acquired = ready(
        poll(&channel, &http, &settings, &code),
        CredentialKind::Oauth,
    );
    assert_eq!(acquired.secret["user_id"], "user-2");
    assert_eq!(
        refresh(&channel, &http, &acquired.secret, &settings)["access_token"],
        "wb-b"
    );
}

#[test]
fn kimi_device_login_carries_device_id_and_refreshes() {
    let channel = crate::KimiChannel;
    let http = MockHttp::new(&[
        (
            200,
            r#"{"device_code":"dc","user_code":"KIMI","verification_uri_complete":"https://kimi.test/device","interval":1}"#,
        ),
        (400, r#"{"error":"authorization_pending"}"#),
        (400, r#"{"error":"access_denied"}"#),
        (
            200,
            r#"{"access_token":"kimi-a","refresh_token":"kimi-r","expires_in":3600}"#,
        ),
        (
            200,
            r#"{"access_token":"kimi-b","refresh_token":"kimi-r2","expires_in":3600}"#,
        ),
    ]);
    let code = start(&channel, &http, &json!({}), &json!({}));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Pending
    ));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Denied
    ));
    let acquired = ready(
        poll(&channel, &http, &json!({}), &code),
        CredentialKind::Oauth,
    );
    assert!(
        acquired.secret["device_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert_eq!(
        refresh(&channel, &http, &acquired.secret, &json!({}))["access_token"],
        "kimi-b"
    );
}

#[test]
fn grok_device_login_and_first_refresh() {
    let channel = crate::GrokBuildChannel;
    let http = MockHttp::new(&[
        (
            200,
            r#"{"device_code":"dc","user_code":"GROK","verification_uri":"https://x.test/device","interval":1}"#,
        ),
        (400, r#"{"error":"authorization_pending"}"#),
        (400, r#"{"error":"access_denied"}"#),
        (
            200,
            r#"{"access_token":"grok-a","refresh_token":"grok-r","expires_in":3600}"#,
        ),
        (
            200,
            r#"{"access_token":"grok-b","refresh_token":"grok-r2","expires_in":3600}"#,
        ),
    ]);
    let code = start(&channel, &http, &json!({}), &json!({}));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Pending
    ));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Denied
    ));
    let acquired = ready(
        poll(&channel, &http, &json!({}), &code),
        CredentialKind::Oauth,
    );
    assert_eq!(
        refresh(&channel, &http, &acquired.secret, &json!({}))["access_token"],
        "grok-b"
    );
}

#[test]
fn opencode_device_login_is_api_key_and_refreshes() {
    let channel = crate::OpenCodeChannel;
    let http = MockHttp::new(&[
        (
            200,
            r#"{"device_code":"dc","user_code":"OPEN","verification_uri":"/device","interval":1}"#,
        ),
        (400, r#"{"error":"authorization_pending"}"#),
        (400, r#"{"error":"access_denied"}"#),
        (
            200,
            r#"{"access_token":"open-a","refresh_token":"open-r","expires_in":3600}"#,
        ),
        (
            200,
            r#"{"access_token":"open-b","refresh_token":"open-r2","expires_in":3600}"#,
        ),
    ]);
    let code = start(&channel, &http, &json!({}), &json!({}));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Pending
    ));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Denied
    ));
    let acquired = ready(
        poll(&channel, &http, &json!({}), &code),
        CredentialKind::ApiKey,
    );
    assert_eq!(acquired.secret["api_key"], "open-a");
    assert_eq!(
        refresh(&channel, &http, &acquired.secret, &json!({}))["api_key"],
        "open-b"
    );
}

#[test]
fn kiro_device_and_sso_logins_preserve_refresh_material() {
    let channel = crate::KiroChannel;
    let http = MockHttp::new(&[
        (
            200,
            r#"{"deviceCode":"dc","userCode":"KIRO","verificationUri":"https://kiro.test","intervalInMilliseconds":1000}"#,
        ),
        (200, r#"{"status":"authorization_pending"}"#),
        (200, r#"{"status":"denied"}"#),
        (
            200,
            r#"{"status":"authorized","accessToken":"kiro-a","refreshToken":"kiro-r","profileArn":"arn:profile"}"#,
        ),
        (
            200,
            r#"{"accessToken":"kiro-b","refreshToken":"kiro-r2","expiresIn":3600}"#,
        ),
        (200, r#"{"clientId":"client-1","clientSecret":"secret-1"}"#),
        (
            200,
            r#"{"accessToken":"sso-a","refreshToken":"sso-r","expiresIn":3600}"#,
        ),
    ]);
    let code = start(
        &channel,
        &http,
        &json!({}),
        &json!({"login_provider":"google"}),
    );
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Pending
    ));
    assert!(matches!(
        poll(&channel, &http, &json!({}), &code),
        DevicePoll::Denied
    ));
    let acquired = ready(
        poll(&channel, &http, &json!({}), &code),
        CredentialKind::Oauth,
    );
    assert_eq!(
        refresh(&channel, &http, &acquired.secret, &json!({}))["access_token"],
        "kiro-b"
    );

    let login = channel.login().unwrap();
    let started = run(login.adapter.authcode_start(
        &http,
        AuthCodeStartCtx {
            provider_settings: &json!({}),
            params: &json!({"auth_method":"idc","start_url":"https://example.awsapps.com/start","region":"eu-west-1"}),
            redirect_uri: "",
            state: "state",
            pkce_challenge: "challenge",
        },
    )).unwrap().unwrap();
    let acquired = run(login.adapter.authcode_exchange(
        &http,
        AuthCodeExchangeCtx {
            provider_settings: &json!({}),
            code: "code",
            verifier: "verifier",
            redirect_uri: &started.redirect_uri,
            extra: started.extra.as_ref(),
        },
    ))
    .unwrap();
    assert_eq!(acquired.secret["client_id"], "client-1");
    assert_eq!(acquired.secret["region"], "eu-west-1");
}

fn google_login_and_refresh(channel: &dyn Channel) {
    let http = MockHttp::new(&[
        (
            200,
            r#"{"access_token":"google-a","refresh_token":"google-r","expires_in":3600}"#,
        ),
        (
            200,
            r#"{"cloudaicompanionProject":{"id":"project-1"},"currentTier":{"id":"free-tier"}}"#,
        ),
        (200, r#"{"email":"user@example.test"}"#),
        (200, r#"{"access_token":"google-b","expires_in":3600}"#),
    ]);
    let login = channel.login().unwrap();
    let started = run(login.adapter.authcode_start(
        &http,
        AuthCodeStartCtx {
            provider_settings: &json!({}),
            params: &json!({}),
            redirect_uri: "",
            state: "state",
            pkce_challenge: "challenge",
        },
    ))
    .unwrap()
    .unwrap();
    let acquired = run(login.adapter.authcode_exchange(
        &http,
        AuthCodeExchangeCtx {
            provider_settings: &json!({}),
            code: "code",
            verifier: "verifier",
            redirect_uri: &started.redirect_uri,
            extra: started.extra.as_ref(),
        },
    ))
    .unwrap();
    assert_eq!(acquired.kind, CredentialKind::Oauth);
    assert_eq!(acquired.secret["project_id"], "project-1");
    assert_eq!(
        refresh(channel, &http, &acquired.secret, &json!({}))["access_token"],
        "google-b"
    );
    assert!(
        http.request_uris()
            .iter()
            .any(|uri| uri.contains("loadCodeAssist"))
    );
}

#[test]
fn antigravity_authcode_login_and_first_refresh() {
    google_login_and_refresh(&crate::AntigravityChannel);
}

#[test]
fn gemini_authcode_login_and_first_refresh() {
    google_login_and_refresh(&crate::GeminiCliChannel);
}
