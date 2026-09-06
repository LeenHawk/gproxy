use base64::Engine as _;
use bytes::Bytes;
use gproxy_channel_api::{GPROXY_OAUTH_SCOPE, PI_OAUTH_CLIENT_ID};
use gproxy_core::{ControlPlane as _, Host as _};
use http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::tests::setup;

#[tokio::test]
async fn account_oauth_enforces_consent_permissions_and_revocation() {
    let fixture = setup::fixture().await;
    let app = &fixture.app;
    let user = app.inner.host.services.control.current().users[0].id;
    let session = browser_session(app, user).await;
    let client = app
        .inner
        .host
        .services
        .store
        .oauth_client(PI_OAUTH_CLIENT_ID)
        .await
        .unwrap()
        .unwrap();
    let verifier = format!("{}{}", super::random(), super::random());
    let authorization = json!({
        "response_type":"code", "client_id":PI_OAUTH_CLIENT_ID,
        "redirect_uri":"http://127.0.0.1:39871/oauth/callback", "scope":GPROXY_OAUTH_SCOPE,
        "code_challenge":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        "code_challenge_method":"S256", "state":super::random(),
    });
    let decision = json!({"authorization":authorization,"approved":true});
    assert_eq!(
        oauth(app, "/oauth/authorize", Some(&session), decision.clone())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    app.inner
        .host
        .services
        .store
        .update_oauth_client(
            client.id,
            &gproxy_store::records::OAuthClientInput {
                client_id: client.client_id,
                name: client.name,
                redirect_uris: client.redirect_uris,
                enabled: true,
            },
            None,
            super::now(),
        )
        .await
        .unwrap();
    assert_eq!(
        oauth(app, "/oauth/authorize", None, decision.clone())
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    for (field, value) in [
        ("redirect_uri", "https://unregistered.invalid/callback"),
        ("code_challenge_method", "plain"),
        ("scope", "admin"),
        ("state", ""),
    ] {
        let mut invalid = decision.clone();
        invalid["authorization"][field] = json!(value);
        assert_eq!(
            oauth(app, "/oauth/authorize", Some(&session), invalid)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut denied = decision.clone();
    denied["approved"] = json!(false);
    let result = response(oauth(app, "/oauth/authorize", Some(&session), denied).await);
    assert!(
        result["redirect_uri"]
            .as_str()
            .unwrap()
            .contains("error=access_denied")
    );
    let result = response(oauth(app, "/oauth/authorize", Some(&session), decision).await);
    let callback = result["redirect_uri"]
        .as_str()
        .unwrap()
        .parse::<http::Uri>()
        .unwrap();
    let pairs = form_urlencoded::parse(callback.query().unwrap().as_bytes())
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(pairs["state"], authorization["state"].as_str().unwrap());
    let mut exchange = json!({"grant_type":"authorization_code", "client_id":PI_OAUTH_CLIENT_ID, "redirect_uri":authorization["redirect_uri"], "code":pairs["code"], "code_verifier":format!("{verifier}bad")});
    assert_eq!(
        oauth(app, "/oauth/token", None, exchange.clone())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    exchange["code_verifier"] = json!(verifier);
    let tokens = response(oauth(app, "/oauth/token", None, exchange.clone()).await);
    assert!(tokens.get("id_token").is_none());
    assert_eq!(
        oauth(app, "/oauth/token", None, exchange).await.status(),
        StatusCode::BAD_REQUEST
    );
    let access = tokens["access_token"].as_str().unwrap();
    let request = setup::request("oauth-permissions", "test", access);
    let identity = app.inner.host.authenticate(&request).await.unwrap();
    let control = &app.inner.host.services.control;
    assert!(!control.catalogue_visible(&identity, Some("public-model"), &request.mode));
    app.mutate(crate::ControlMutation::Permission(
        gproxy_store::records::PermissionInput {
            subject_kind: "user".into(),
            subject_id: user,
            provider_id: Some(fixture.provider),
            operation_group: Some("generate_content".into()),
            allowed: true,
        },
    ))
    .await
    .unwrap();
    assert!(control.catalogue_visible(&identity, Some("public-model"), &request.mode));
    let plan = control
        .resolve_preprocessed(Some("public-model"), &request.mode, None)
        .unwrap();
    let operation = crate::tests::generation_operation();
    app.inner
        .host
        .admit(&identity, &request, Some(operation), &plan)
        .await
        .unwrap();
    app.inner
        .host
        .finish_admission(&request.request_id, None)
        .await;
    assert!(
        app.inner
            .host
            .admit(&identity, &request, None, &plan)
            .await
            .is_err()
    );
    let mut parts = parts("/portal/api/oauth-sessions", None, Method::GET);
    parts.headers.insert(
        http::header::AUTHORIZATION,
        format!("Bearer {access}").parse().unwrap(),
    );
    assert_eq!(
        app.portal_dispatch(&parts, Bytes::new())
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let page = app
        .inner
        .host
        .services
        .store
        .oauth_sessions(user, super::now(), false, 20, 0)
        .await
        .unwrap();
    assert_eq!((page.total_logins, page.active_sessions), (1, 1));
    let path = format!("/portal/api/oauth-sessions/{}", page.sessions[0].id);
    let mut delete = self::parts(&path, Some(&session), Method::DELETE);
    delete.headers.remove(http::header::ORIGIN);
    assert_eq!(
        app.portal_dispatch(&delete, Bytes::new())
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let delete = self::parts(&path, Some(&session), Method::DELETE);
    assert_eq!(
        app.portal_dispatch(&delete, Bytes::new())
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert!(app.inner.host.authenticate(&request).await.is_err());
    assert!(
        app.inner
            .host
            .admit(&identity, &request, Some(operation), &plan)
            .await
            .is_err()
    );
    assert_eq!(oauth(app, "/oauth/token", None, json!({"grant_type":"refresh_token", "client_id":PI_OAUTH_CLIENT_ID,"refresh_token":tokens["refresh_token"]})).await.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn standard_device_grant_counts_only_successful_token_exchange() {
    let fixture = setup::fixture().await;
    let app = &fixture.app;
    let user = app.inner.host.services.control.current().users[0].id;
    let session = browser_session(app, user).await;
    let client = app
        .inner
        .host
        .services
        .store
        .oauth_client(PI_OAUTH_CLIENT_ID)
        .await
        .unwrap()
        .unwrap();
    app.inner
        .host
        .services
        .store
        .update_oauth_client(
            client.id,
            &gproxy_store::records::OAuthClientInput {
                client_id: client.client_id,
                name: client.name,
                redirect_uris: client.redirect_uris,
                enabled: true,
            },
            None,
            super::now(),
        )
        .await
        .unwrap();
    for approve in [false, true] {
        let device = response(
            oauth(
                app,
                "/oauth/device/code",
                None,
                json!({"client_id":PI_OAUTH_CLIENT_ID,"scope":GPROXY_OAUTH_SCOPE}),
            )
            .await,
        );
        let poll = json!({"grant_type":"urn:ietf:params:oauth:grant-type:device_code", "client_id":PI_OAUTH_CLIENT_ID,"device_code":device["device_code"]});
        let pending = oauth(app, "/oauth/token", None, poll.clone()).await;
        assert_eq!(
            serde_json::from_slice::<Value>(pending.body()).unwrap()["error"],
            "authorization_pending"
        );
        assert_eq!(
            oauth(
                app,
                "/oauth/device/decision",
                Some(&session),
                json!({"user_code":device["user_code"],"approved":approve})
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.inner
                .host
                .services
                .store
                .oauth_sessions(user, super::now(), false, 20, 0)
                .await
                .unwrap()
                .total_logins,
            0
        );
        let completed = oauth(app, "/oauth/token", None, poll.clone()).await;
        if approve {
            let tokens = response(completed);
            assert!(tokens.get("id_token").is_none());
            let refresh = json!({"grant_type":"refresh_token","client_id":PI_OAUTH_CLIENT_ID,"refresh_token":tokens["refresh_token"]});
            let (left, right) = tokio::join!(
                oauth(app, "/oauth/token", None, refresh.clone()),
                oauth(app, "/oauth/token", None, refresh)
            );
            assert_ne!(left.status().is_success(), right.status().is_success());
            let page = app
                .inner
                .host
                .services
                .store
                .oauth_sessions(user, super::now(), false, 20, 0)
                .await
                .unwrap();
            assert_eq!(
                (
                    page.total_logins,
                    page.active_sessions,
                    page.sessions[0].refresh_count
                ),
                (1, 1, Some(1))
            );
            assert_eq!(
                oauth(app, "/oauth/token", None, poll).await.status(),
                StatusCode::BAD_REQUEST
            );
        } else {
            assert_eq!(
                serde_json::from_slice::<Value>(completed.body()).unwrap()["error"],
                "access_denied"
            );
        }
    }
}

async fn browser_session(app: &crate::AppHandle, user_id: i64) -> String {
    let session = super::random();
    app.inner
        .host
        .services
        .store
        .set_user_password(user_id, &super::random())
        .await
        .unwrap();
    app.inner
        .host
        .services
        .store
        .create_user_session(&gproxy_store::records::UserSessionInput {
            token_digest: Sha256::digest(session.as_bytes()).to_vec(),
            user_id,
            created_at: super::now(),
            expires_at: super::now() + 3600,
        })
        .await
        .unwrap();
    session
}

fn parts(path: &str, session: Option<&str>, method: Method) -> http::request::Parts {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "gproxy.test")
        .header("origin", "http://gproxy.test")
        .header("content-type", "application/json");
    if let Some(session) = session {
        request = request.header("cookie", format!("gproxy_portal_session={session}"));
    }
    request.body(()).unwrap().into_parts().0
}

async fn oauth(
    app: &crate::AppHandle,
    path: &str,
    session: Option<&str>,
    body: Value,
) -> http::Response<Bytes> {
    app.oauth_dispatch(
        &parts(path, session, Method::POST),
        Bytes::from(body.to_string()),
    )
    .await
    .unwrap()
}

fn response(result: http::Response<Bytes>) -> Value {
    assert_eq!(result.status(), StatusCode::OK);
    serde_json::from_slice(result.body()).unwrap()
}
