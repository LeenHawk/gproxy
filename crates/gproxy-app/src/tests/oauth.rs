use base64::Engine as _;
use bytes::Bytes;
use gproxy_core::{Host as _, RequestCtx, RoutingMode};
use http::{HeaderMap, HeaderValue, Method};
use sha2::{Digest, Sha256};

use crate::{App, Config, ControlMutation, MasterKeyConfig};

#[tokio::test]
async fn named_codex_oauth_issues_an_authenticating_access_token() {
    let directory = tempfile::tempdir().unwrap();
    let app = App::start(Config::sqlite(
        "127.0.0.1:0".parse().unwrap(),
        directory.path().into(),
        MasterKeyConfig::new(Some([9; 32])),
    ))
    .await
    .unwrap();
    let provider = super::setup::id(
        app.mutate(ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "codex".into(),
                label: None,
                channel: "codex".into(),
                settings: serde_json::json!({}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: true,
            },
        ))
        .await
        .unwrap(),
    );
    app.mutate(ControlMutation::Credential {
        provider_id: provider,
        label: None,
        secret: serde_json::json!({"access_token":random(),"account_id":"upstream"}),
        enabled: true,
    })
    .await
    .unwrap();
    let user = super::setup::id(
        app.mutate(ControlMutation::User(gproxy_store::records::UserInput {
            name: "oauth-user".into(),
            organization_id: None,
            team_id: None,
            password_hash: Some("fixture".into()),
            enabled: true,
            is_admin: false,
        }))
        .await
        .unwrap(),
    );
    app.mutate(ControlMutation::Permission(
        gproxy_store::records::PermissionInput {
            subject_kind: "user".into(),
            subject_id: user,
            provider_id: Some(provider),
            operation_group: Some("generate_content".into()),
            allowed: true,
        },
    ))
    .await
    .unwrap();
    let session = random();
    app.inner
        .host
        .services
        .store
        .create_user_session(&gproxy_store::records::UserSessionInput {
            token_digest: Sha256::digest(session.as_bytes()).to_vec(),
            user_id: user,
            created_at: now(),
            expires_at: now() + 3600,
        })
        .await
        .unwrap();

    let verifier = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-._~";
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    let query = serde_urlencoded::to_string([
        ("response_type", "code"),
        ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
        ("redirect_uri", "http://localhost:1455/auth/callback"),
        (
            "scope",
            "openid profile email offline_access api.connectors.read api.connectors.invoke",
        ),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", "state-1"),
    ])
    .unwrap();
    let form = format!("{query}&decision=approve");
    let authorize_request = request(
        &app,
        Method::POST,
        "/codex/oauth/authorize",
        Some(&session),
        Some("application/x-www-form-urlencoded"),
        Bytes::from(form),
    );
    let outcome = app.execute(authorize_request).await.unwrap();
    assert_eq!(outcome.status, http::StatusCode::FOUND);
    let location = outcome.headers[http::header::LOCATION].to_str().unwrap();
    let callback = location.parse::<http::Uri>().unwrap();
    let pairs = form_urlencoded::parse(callback.query().unwrap().as_bytes())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    let code = pairs
        .iter()
        .find_map(|(name, value)| (name == "code").then(|| value.clone()))
        .unwrap();
    assert_eq!(
        pairs
            .iter()
            .find_map(|(name, value)| (name == "state").then_some(value.as_str())),
        Some("state-1")
    );

    let token_body = serde_urlencoded::to_string([
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", "http://localhost:1455/auth/callback"),
        ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
        ("code_verifier", verifier),
    ])
    .unwrap();
    let token = app
        .execute(request(
            &app,
            Method::POST,
            "/codex/oauth/token",
            None,
            Some("application/x-www-form-urlencoded"),
            Bytes::from(token_body),
        ))
        .await
        .unwrap();
    assert_eq!(token.status, http::StatusCode::OK);
    let gproxy_core::ResponseBody::Full(body) = token.body else {
        panic!("token response was not buffered")
    };
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let access = body["access_token"].as_str().unwrap();
    let refresh = body["refresh_token"].as_str().unwrap();
    assert_eq!(access.split('.').count(), 3);

    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access}")).unwrap(),
    );
    let identity = app
        .inner
        .host
        .authenticate(&RequestCtx {
            request_id: "access-auth".into(),
            client_ip: None,
            method: Method::GET,
            path: "/v1/models".into(),
            query: None,
            headers,
            body: Bytes::new(),
            upgrade: false,
            mode: RoutingMode::Aggregated,
        })
        .await
        .unwrap();
    assert_eq!(identity.user_id, user);
    assert_ne!(identity.user_key_id, 0);
    let oauth_key_id = identity.user_key_id;

    let refreshed = app
        .execute(request(
            &app,
            Method::POST,
            "/codex/oauth/token",
            None,
            Some("application/json"),
            Bytes::from(
                serde_json::json!({
                    "grant_type":"refresh_token",
                    "refresh_token":refresh,
                    "client_id":"app_EMoamEEZ73f0CkXaXp7hrann"
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(refreshed.status, http::StatusCode::OK);
    let reused = app
        .execute(request(
            &app,
            Method::POST,
            "/codex/oauth/token",
            None,
            Some("application/json"),
            Bytes::from(
                serde_json::json!({
                    "grant_type":"refresh_token",
                    "refresh_token":refresh,
                    "client_id":"app_EMoamEEZ73f0CkXaXp7hrann"
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(reused.status, http::StatusCode::BAD_REQUEST);

    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access}")).unwrap(),
    );
    let method = Method::GET;
    let (mode, path) =
        crate::ingress::normalize_path(&app, &method, "/codex/backend-api/codex/responses", true);
    let outcome = app
        .execute(RequestCtx {
            request_id: "responses-ws".into(),
            client_ip: None,
            method,
            path,
            query: None,
            headers,
            body: Bytes::new(),
            upgrade: true,
            mode,
        })
        .await
        .unwrap();
    let gproxy_core::ResponseBody::WebSocket(mut socket) = outcome.body else {
        panic!("Responses upgrade did not return a websocket")
    };
    socket
        .send(gproxy_core::WsFrame::Text(
            serde_json::json!({"type":"response.create","generate":false}).to_string(),
        ))
        .await
        .unwrap();
    let Some(gproxy_core::WsFrame::Text(event)) = socket.recv().await.unwrap() else {
        panic!("warmup did not produce a websocket event")
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&event).unwrap()["type"],
        "response.completed"
    );

    let device = app
        .execute(request(
            &app,
            Method::POST,
            "/codex/api/accounts/deviceauth/usercode",
            None,
            Some("application/json"),
            Bytes::from(
                serde_json::json!({"client_id":"app_EMoamEEZ73f0CkXaXp7hrann"}).to_string(),
            ),
        ))
        .await
        .unwrap();
    let gproxy_core::ResponseBody::Full(device_body) = device.body else {
        panic!("device start was not buffered")
    };
    let device: serde_json::Value = serde_json::from_slice(&device_body).unwrap();
    let device_auth_id = device["device_auth_id"].as_str().unwrap();
    let user_code = device["user_code"].as_str().unwrap();
    assert_eq!(device["interval"].as_str(), Some("5"));
    let approve =
        serde_urlencoded::to_string([("user_code", user_code), ("decision", "approve")]).unwrap();
    assert_eq!(
        app.execute(request(
            &app,
            Method::POST,
            "/codex/codex/device",
            Some(&session),
            Some("application/x-www-form-urlencoded"),
            Bytes::from(approve),
        ))
        .await
        .unwrap()
        .status,
        http::StatusCode::OK
    );
    let poll = app
        .execute(request(
            &app,
            Method::POST,
            "/codex/api/accounts/deviceauth/token",
            None,
            Some("application/json"),
            Bytes::from(
                serde_json::json!({
                    "device_auth_id":device_auth_id,
                    "user_code":user_code
                })
                .to_string(),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(poll.status, http::StatusCode::OK);
    let gproxy_core::ResponseBody::Full(poll_body) = poll.body else {
        panic!("device poll was not buffered")
    };
    let poll: serde_json::Value = serde_json::from_slice(&poll_body).unwrap();
    assert!(poll["authorization_code"].as_str().is_some());
    assert!(poll["code_verifier"].as_str().is_some());

    let revoked = app
        .execute(request(
            &app,
            Method::POST,
            "/codex/oauth/revoke",
            None,
            Some("application/json"),
            Bytes::from(serde_json::json!({"token":access}).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(revoked.status, http::StatusCode::OK);
    let snapshot = app
        .inner
        .host
        .services
        .store
        .control_snapshot()
        .await
        .unwrap();
    assert!(
        !snapshot
            .user_keys
            .iter()
            .find(|key| key.id == oauth_key_id)
            .unwrap()
            .enabled
    );
}

fn request(
    app: &crate::AppHandle,
    method: Method,
    path: &str,
    session: Option<&str>,
    content_type: Option<&str>,
    body: Bytes,
) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert(http::header::HOST, HeaderValue::from_static("gproxy.test"));
    if let Some(session) = session {
        headers.insert(
            http::header::COOKIE,
            HeaderValue::from_str(&format!("gproxy_portal_session={session}")).unwrap(),
        );
    }
    if let Some(content_type) = content_type {
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_str(content_type).unwrap(),
        );
    }
    let (mode, normalized) = crate::ingress::normalize_path(app, &method, path, false);
    RequestCtx {
        request_id: random(),
        client_ip: None,
        method,
        path: normalized,
        query: (path.ends_with("authorize")).then(|| {
            String::from_utf8(body.clone().to_vec())
                .unwrap()
                .split("&decision=")
                .next()
                .unwrap()
                .to_owned()
        }),
        headers,
        body,
        upgrade: false,
        mode,
    }
}

fn random() -> String {
    let mut bytes = [0_u8; 18];
    getrandom::fill(&mut bytes).unwrap();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
