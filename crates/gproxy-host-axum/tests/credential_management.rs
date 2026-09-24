#![cfg(not(target_arch = "wasm32"))]
mod support;

use gproxy_app::{App, AppConfig};
use gproxy_channel::{
    BaseChannel,
    channel::{
        AcquiredCredential, AuthorizationCode, AuthorizationRequest, AuthorizationStart,
        CookieLogin, DeviceAuthorization, DevicePoll, LoginContext, OAuthAuthorizationCode,
        OAuthCredential, OAuthDeviceCode, OperationFuture,
    },
};
use gproxy_host_axum::HostState;
use gproxy_sdk::{ClientPool, GproxyBuilder, SyncMode};
use http::StatusCode;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
use support::{Host, ScriptClient, get, keyed, post};

#[derive(Default)]
struct LoginChannel {
    polls: Mutex<VecDeque<DevicePoll>>,
}
impl BaseChannel for LoginChannel {
    fn id(&self) -> &'static str {
        "test"
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }
    fn cookie_login(&self) -> Option<&dyn CookieLogin> {
        Some(self)
    }
}
fn token() -> OAuthCredential {
    OAuthCredential {
        access_token: "never-in-response".into(),
        refresh_token: Some("refresh-secret".into()),
        id_token: None,
        token_type: None,
        scopes: vec![],
        expires_at_ms: None,
        refresh_expires_at_ms: None,
        provider_fields: BTreeMap::new(),
        provider_secrets: BTreeMap::new(),
    }
}
impl OAuthAuthorizationCode for LoginChannel {
    fn authorize<'a>(
        &'a self,
        _: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            Ok(AuthorizationStart {
                authorize_url: format!("https://auth.example/?state={}", request.state),
                redirect_uri: "http://localhost:1234/callback".into(),
                provider_state: BTreeMap::new(),
            })
        })
    }
    fn exchange<'a>(
        &'a self,
        _: LoginContext<'a>,
        _: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async { Ok(token()) })
    }
}
impl OAuthDeviceCode for LoginChannel {
    fn start<'a>(&'a self, _: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async {
            Ok(DeviceAuthorization {
                device_code: "secret-device-handle".into(),
                user_code: "CODE".into(),
                verification_uri: "https://auth.example/device".into(),
                verification_uri_complete: None,
                expires_at_ms: None,
                interval_secs: 2,
                provider_state: BTreeMap::new(),
            })
        })
    }
    fn poll<'a>(
        &'a self,
        _: LoginContext<'a>,
        _: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        let result = self
            .polls
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(DevicePoll::Pending);
        Box::pin(async move { Ok(result) })
    }
}
impl CookieLogin for LoginChannel {
    fn exchange_cookie<'a>(
        &'a self,
        _: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, AcquiredCredential> {
        Box::pin(async move {
            Ok(AcquiredCredential {
                secret: json!({ "cookie": cookie }),
                expires_at_ms: None,
                metadata: json!({}),
            })
        })
    }
}
async fn instance() -> (Host, Arc<LoginChannel>) {
    let channel = Arc::new(LoginChannel::default());
    let client = Arc::new(ScriptClient::default());
    let handle = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .plaintext_secrets()
        .without_default_channels()
        .channel(channel.clone())
        .client_pool(ClientPool::with_client(client.clone()))
        .cache(Arc::new(
            gproxy_cache::MemoryCache::new(gproxy_cache::MemoryOptions::default()).unwrap(),
        ))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();
    let app = Arc::new(App::new(handle, AppConfig::default()));
    let router = gproxy_host_axum::router(HostState::new(app.clone()));
    let host = Host {
        app,
        router,
        client,
    };
    let handle = host.handle();
    for id in ["root", "other"] {
        support::person(&handle, id, "admin").await;
        support::api_key(&handle, &format!("k-{id}"), id, None, None).await;
    }
    support::person(&handle, "tenant", "user").await;
    for org in ["a", "b"] {
        support::organization(&handle, org).await;
        support::api_key(&handle, &format!("k-{org}"), "tenant", Some(org), None).await;
    }
    support::team(&handle, "child", "a").await;
    support::api_key(&handle, "k-plain", "tenant", None, None).await;
    support::provider(&handle, "p", &["m"]).await;
    support::credential(&handle, "ca", "p", None, None, Some("a")).await;
    support::credential(&handle, "cb", "p", None, None, Some("b")).await;
    host.publish().await;
    (host, channel)
}
async fn login(host: &Host, key: &str, path: &str, body: Value) -> support::Answer {
    host.send(keyed(
        post(&format!("/admin/api/credential-login/{path}"), body),
        key,
    ))
    .await
}

#[tokio::test]
async fn scoped_directories_and_probes_do_not_open_gateway_configuration() {
    let (host, _) = instance().await;
    let providers = host
        .send(keyed(get("/admin/api/credentials/providers"), "k-a"))
        .await;
    assert_eq!(providers.status, StatusCode::OK, "{}", providers.text());
    let provider = &providers.json()[0];
    assert_eq!(provider["id"], "p");
    assert!(provider.get("config").is_none());
    assert!(provider.get("baseUrl").is_none());
    let owners = host
        .send(keyed(get("/admin/api/credentials/owners"), "k-a"))
        .await;
    assert_eq!(owners.status, StatusCode::OK);
    assert_eq!(owners.json().as_array().unwrap().len(), 2);
    assert!(!owners.text().contains("\"b\""));
    assert_eq!(
        host.send(keyed(get("/admin/api/providers"), "k-a"))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        host.send(keyed(get("/admin/api/credentials/providers"), "k-plain"))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    for endpoint in ["models/discover", "models/test"] {
        let result = host
            .send(keyed(
                post(
                    &format!("/admin/api/credentials/cb/{endpoint}"),
                    json!({"model":"m"}),
                ),
                "k-a",
            ))
            .await;
        assert_eq!(result.status, StatusCode::NOT_FOUND, "{}", result.text());
    }
}

#[tokio::test]
async fn authorization_binds_user_scope_owner_and_requires_state() {
    let (host, _) = instance().await;
    let start = login(
        &host,
        "k-a",
        "authcode/start",
        json!({"providerId":"p", "owner":{"organizationId":"a"}}),
    )
    .await;
    assert_eq!(start.status, StatusCode::OK, "{}", start.text());
    let data = start.json();
    let session = data["loginSessionId"].as_str().unwrap();
    let state = data["authorizeUrl"]
        .as_str()
        .unwrap()
        .split("state=")
        .nth(1)
        .unwrap();
    let body = json!({"loginSessionId":session,"callbackUrl":format!("http://localhost:1234/callback?code=ok&state={state}")});
    for key in ["k-b", "k-root", "k-other"] {
        let result = login(&host, key, "authcode/complete", body.clone()).await;
        assert_eq!(result.status, StatusCode::NOT_FOUND, "{}", result.text());
    }
    let missing = login(
        &host,
        "k-a",
        "authcode/complete",
        json!({"loginSessionId":session,"callbackUrl":"http://localhost:1234/callback?code=ok"}),
    )
    .await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    let result = login(&host, "k-a", "authcode/complete", body.clone()).await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    assert!(!result.text().contains("never-in-response"));
    let id = result.json()["credentialId"].as_str().unwrap().to_owned();
    let row = host
        .send(keyed(get(&format!("/admin/api/credentials/{id}")), "k-a"))
        .await;
    assert_eq!(row.json()["organizationId"], "a");
    assert_eq!(
        login(&host, "k-a", "authcode/complete", body).await.status,
        StatusCode::NOT_FOUND
    );
    let cross = login(
        &host,
        "k-a",
        "cookie/exchange",
        json!({"providerId":"p","cookie":"private","owner":{"organizationId":"b"}}),
    )
    .await;
    assert_eq!(cross.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn device_flow_preserves_poll_cadence_and_terminal_results() {
    let (host, channel) = instance().await;
    for terminal in [
        DevicePoll::Denied,
        DevicePoll::Expired,
        DevicePoll::Ready(token()),
    ] {
        let started = login(
            &host,
            "k-a",
            "device/start",
            json!({"providerId":"p","owner":{"teamId":"child"}}),
        )
        .await;
        assert_eq!(started.status, StatusCode::OK, "{}", started.text());
        assert!(!started.text().contains("secret-device-handle"));
        let body = json!({"loginSessionId":started.json()["loginSessionId"]});
        channel.polls.lock().unwrap().extend([
            DevicePoll::SlowDown { interval_secs: 7 },
            DevicePoll::Pending,
            terminal,
        ]);
        let slow = login(&host, "k-a", "device/poll", body.clone()).await;
        assert_eq!(slow.json(), json!({"status":"pending","intervalSecs":7}));
        let pending = login(&host, "k-a", "device/poll", body.clone()).await;
        assert_eq!(pending.json(), json!({"status":"pending","intervalSecs":7}));
        let done = login(&host, "k-a", "device/poll", body.clone()).await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.text());
        assert_ne!(done.json()["status"], "pending");
        assert_eq!(
            login(&host, "k-a", "device/poll", body).await.status,
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn wrong_state_invalidates_authorization_and_cookie_login_stays_scoped() {
    let (host, _) = instance().await;
    let start = login(
        &host,
        "k-a",
        "authcode/start",
        json!({"providerId":"p","owner":{"organizationId":"a"}}),
    )
    .await;
    let id = start.json()["loginSessionId"].clone();
    let result = login(
        &host,
        "k-a",
        "authcode/complete",
        json!({"loginSessionId":id,"callbackUrl":"http://localhost/callback?code=x&state=wrong"}),
    )
    .await;
    assert_eq!(result.status, StatusCode::BAD_REQUEST);
    let result = login(
        &host,
        "k-a",
        "cookie/exchange",
        json!({"providerId":"p","cookie":"private-cookie","owner":{"teamId":"child"}}),
    )
    .await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    assert!(!result.text().contains("private-cookie"));
    let id = result.json()["credentialId"].as_str().unwrap().to_owned();
    let row = host
        .send(keyed(get(&format!("/admin/api/credentials/{id}")), "k-a"))
        .await;
    assert_eq!(row.json()["teamId"], "child");
    assert_eq!(
        host.send(keyed(get(&format!("/admin/api/credentials/{id}")), "k-b"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn imported_settings_reload_and_a_bad_document_does_not_partially_write() {
    let (host, _) = instance().await;
    let exported = host
        .send(keyed(
            post("/admin/api/export", json!({"includeSecrets":false})),
            "k-root",
        ))
        .await;
    assert_eq!(exported.status, StatusCode::OK);
    let mut document = exported.json();
    document["data"]["settings"]["instance"]["instanceName"] = json!("Imported gateway");
    document["data"]["settings"]["instance"]["corsOrigins"] = json!(["https://imported.example"]);
    let imported = host
        .send(keyed(
            post(
                "/admin/api/import",
                json!({"export":document,"mode":"merge"}),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.text());
    let info = host.send(get("/info")).await;
    assert_eq!(info.json()["instanceName"], "Imported gateway");
    assert_eq!(
        gproxy_host_axum::runtime_settings::cors_origins(&host.app),
        vec!["https://imported.example"]
    );
    let before = host
        .send(keyed(get("/admin/api/settings"), "k-root"))
        .await
        .json();
    document["data"]["settings"]["instance"]["instanceName"] = json!("Must not land");
    document["data"]["routeMembers"] = json!([{"id":"bad-member","routeId":"missing-route","providerId":"p","upstreamModel":"m","tier":0,"weight":1,"enabled":true}]);
    let refused = host
        .send(keyed(
            post(
                "/admin/api/import",
                json!({"export":document,"mode":"merge"}),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    let after = host
        .send(keyed(get("/admin/api/settings"), "k-root"))
        .await
        .json();
    assert_eq!(before, after);
}

#[tokio::test]
async fn disabled_providers_cannot_start_or_complete_an_upstream_login() {
    let (host, _) = instance().await;
    let started = login(
        &host,
        "k-a",
        "device/start",
        json!({"providerId":"p", "owner":{"organizationId":"a"}}),
    )
    .await;
    assert_eq!(started.status, StatusCode::OK);
    host.handle()
        .manage()
        .providers()
        .update(
            "p",
            gproxy_sdk::dto::ProviderPatch {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        login(
            &host,
            "k-a",
            "device/poll",
            json!({"loginSessionId":started.json()["loginSessionId"]})
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        login(
            &host,
            "k-a",
            "cookie/exchange",
            json!({"providerId":"p","cookie":"private","owner":{"organizationId":"a"}})
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let directory = host
        .send(keyed(get("/admin/api/credentials/providers"), "k-a"))
        .await;
    assert_eq!(directory.json()[0]["id"], "p");
    assert_eq!(directory.json()[0]["enabled"], false);
}
