use bytes::Bytes;
use http::{Method, StatusCode};

use super::helpers::{parts, state};

#[tokio::test]
async fn portal_reveal_requires_session_ownership_and_same_origin() {
    let state = state().await;
    let owner = crate::seed_first_admin(&state.store, "owner", "test-password")
        .await
        .unwrap()
        .unwrap();
    let login = crate::portal_dispatch(
        &state,
        &parts(Method::POST, "/portal/api/login", None),
        Bytes::from_static(br#"{"username":"owner","password":"test-password"}"#),
    )
    .await
    .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login.headers()[http::header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let created = crate::portal_dispatch(
        &state,
        &parts(Method::POST, "/portal/api/keys", Some(&cookie)),
        Bytes::from_static(br#"{"prefix":"sk","label":null,"expires_at":null}"#),
    )
    .await
    .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: crate::dto::UserKeyCreateResponse =
        serde_json::from_slice(created.body()).unwrap();
    let path = format!("/portal/api/keys/{}/reveal", created.id);
    let list = crate::portal_dispatch(
        &state,
        &parts(Method::GET, "/portal/api/keys", Some(&cookie)),
        Bytes::new(),
    )
    .await
    .unwrap();
    let list: serde_json::Value = serde_json::from_slice(list.body()).unwrap();
    assert_eq!(list[0]["revealable"], true);
    assert!(list[0].get("api_key").is_none());
    for _ in 0..2 {
        let response = crate::portal_dispatch(
            &state,
            &parts(Method::POST, &path, Some(&cookie)),
            Bytes::new(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[http::header::CACHE_CONTROL], "no-store");
    }
    let anonymous = crate::portal_dispatch(&state, &parts(Method::POST, &path, None), Bytes::new())
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let mut cross_origin = parts(Method::POST, &path, Some(&cookie));
    cross_origin.headers.insert(
        http::header::ORIGIN,
        "https://foreign.example".parse().unwrap(),
    );
    cross_origin
        .headers
        .insert(http::header::HOST, "localhost".parse().unwrap());
    let response = crate::portal_dispatch(&state, &cross_origin, Bytes::new())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let other = state
        .store
        .insert_user(&gproxy_store::records::UserInput {
            name: "other".into(),
            is_admin: false,
            password_hash: None,
            organization_id: None,
            team_id: None,
            enabled: true,
        })
        .await
        .unwrap();
    let foreign = state
        .store
        .insert_user_key(&gproxy_store::records::UserKeyInput {
            user_id: other,
            digest: vec![0; 32],
            digest_version: 1,
            prefix: "hidden".into(),
            envelope: super::helpers::envelope(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .unwrap();
    assert_ne!(owner, other);
    for id in [foreign, i64::MAX] {
        let path = format!("/portal/api/keys/{id}/reveal");
        let response = crate::portal_dispatch(
            &state,
            &parts(Method::POST, &path, Some(&cookie)),
            Bytes::new(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    state
        .store
        .insert_oauth_grant(&gproxy_store::records::OAuthGrantInput {
            user_id: owner,
            user_key_id: created.id,
            provider_id: None,
            client_id: "portal-test".into(),
            scopes: "api".into(),
            chatgpt_user_id: "test-user".into(),
            chatgpt_account_id: "test-account".into(),
            created_at: 0,
        })
        .await
        .unwrap();
    let response = crate::portal_dispatch(
        &state,
        &parts(Method::POST, &path, Some(&cookie)),
        Bytes::new(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
