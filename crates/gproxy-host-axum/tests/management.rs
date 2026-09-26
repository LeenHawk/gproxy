#![cfg(not(target_arch = "wasm32"))]
//! The two management surfaces through the router: `/admin/api` and
//! `/portal/api`.
//!
//! The properties these assert are the middleware's, not the operations' —
//! the operations have their own suite in `gproxy-app`. What is tested here is
//! the binding: that a request with no credential is refused before an
//! operation runs, that an ordinary account cannot reach the operator's
//! surface, that a cookie caller is same-origin checked and a key caller is
//! not, that a write leaves an audit row, and that a round trip through the
//! real HTTP shapes produces the rows the DTOs describe.

mod support;

use gproxy_app::{AppConfig, dto::UserWrite};
use gproxy_store::entity::identity::audit_event;
use http::{Method, StatusCode};
use sea_orm::EntityTrait;
use serde_json::json;
use support::{Host, get, keyed, post, request, with};

/// One administrator and one ordinary account, both with keys and passwords.
async fn instance() -> Host {
    let host = Host::with_config(AppConfig {
        cors_origins: vec!["https://console.example.com".into()],
        ..AppConfig::default()
    })
    .await;
    let handle = host.handle();
    support::person(&handle, "root", "admin").await;
    support::api_key(&handle, "k-root", "root", None, None).await;
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    host.publish().await;

    let data = host.data();
    host.operations(&data)
        .users()
        .set_password("alice", "correct horse battery")
        .await
        .unwrap();
    drop(data);
    host.publish().await;
    host
}

async fn audit_rows(host: &Host) -> Vec<audit_event::Model> {
    host.app
        .gproxy()
        .store()
        .audit_events()
        .query(audit_event::Entity::find())
        .await
        .unwrap()
}

// -------------------------------------------------------------- /admin/api --

#[tokio::test]
async fn retired_subscription_families_are_not_exposed() {
    let host = instance().await;
    let context = host.send(keyed(get("/admin/api/context"), "k-root")).await;
    assert_eq!(context.status, StatusCode::OK);
    let context = context.json();
    for family in [
        "plans",
        "plan-limits",
        "subscriptions",
        "pools",
        "pool-members",
    ] {
        for method in [Method::GET, Method::POST] {
            let answer = host
                .send(keyed(
                    request(method, &format!("/admin/api/{family}")),
                    "k-root",
                ))
                .await;
            assert_eq!(
                answer.status,
                StatusCode::NOT_FOUND,
                "{family}: {}",
                answer.text()
            );
        }
        assert!(
            context["sections"]
                .as_array()
                .unwrap()
                .iter()
                .all(|section| section["id"] != family)
        );
    }
}

#[tokio::test]
async fn the_operator_surface_needs_a_credential_and_an_instance_admin() {
    let host = instance().await;

    let answer = host.send(get("/admin/api/users")).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    assert_eq!(answer.json()["error"]["code"], "unauthorized");

    // A real, valid key that is not an instance administrator's.
    let answer = host.send(keyed(get("/admin/api/users"), "k-alice")).await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.json()["error"]["code"], "forbidden");

    let answer = host.send(keyed(get("/admin/api/users"), "k-root")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert!(answer.json()["items"].is_array());
}

#[tokio::test]
async fn an_admin_crud_round_trip_goes_through_the_router() {
    let host = instance().await;

    // Create.
    let created = host
        .send(keyed(
            post(
                "/admin/api/users",
                json!({ "name": "carol", "role": "user" }),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_owned();
    assert_eq!(created.json()["name"], "carol");
    // The digest columns have no DTO field at all.
    assert!(created.json().get("passwordHash").is_none());

    // Read.
    let read = host
        .send(keyed(get(&format!("/admin/api/users/{id}")), "k-root"))
        .await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.json()["name"], "carol");

    // Update. `POST` on an item route is not declared, so axum answers 405
    // rather than falling through to the ingress fallback.
    let patched = host
        .send(keyed(
            post(
                &format!("/admin/api/users/{id}"),
                json!({ "enabled": false }),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(patched.status, StatusCode::METHOD_NOT_ALLOWED);

    let mut patch = post(
        &format!("/admin/api/users/{id}"),
        json!({ "enabled": false }),
    );
    *patch.method_mut() = Method::PATCH;
    let patched = host.send(keyed(patch, "k-root")).await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text());
    assert_eq!(patched.json()["enabled"], false);

    // List, and find it.
    let listed = host.send(keyed(get("/admin/api/users"), "k-root")).await;
    let listed = listed.json();
    let names: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"carol"), "{names:?}");

    // Delete, which answers 204 rather than an empty object.
    let deleted = host
        .send(keyed(
            request(Method::DELETE, &format!("/admin/api/users/{id}")),
            "k-root",
        ))
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert!(deleted.bytes.is_empty());

    // And a second delete is a 404, not a silent success.
    let again = host
        .send(keyed(
            request(Method::DELETE, &format!("/admin/api/users/{id}")),
            "k-root",
        ))
        .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);

    // Every non-channel operation, including reads, leaves an audit row.
    let actions: Vec<String> = audit_rows(&host)
        .await
        .into_iter()
        .map(|row| row.action)
        .collect();
    assert!(
        actions.contains(&"admin.users.create".to_owned()),
        "{actions:?}"
    );
    assert!(
        actions.contains(&"admin.users.update".to_owned()),
        "{actions:?}"
    );
    assert!(
        actions.contains(&"admin.users.delete".to_owned()),
        "{actions:?}"
    );
    assert!(
        actions.iter().any(|action| action.ends_with(".list")),
        "non-channel reads must be audited: {actions:?}"
    );
}

#[tokio::test]
async fn an_unknown_admin_path_is_404_without_touching_the_database() {
    let host = instance().await;
    // No credential at all: the guard is a `route_layer`, so an unmatched path
    // never reaches it.
    let answer = host.send(get("/admin/api/nope")).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
}

// ------------------------------------------------------------------ csrf --

#[tokio::test]
async fn a_cookie_write_is_same_origin_checked_and_a_key_write_is_not() {
    let host = instance().await;

    // Sign in to get a cookie.
    let login = host
        .send(post(
            "/portal/api/login",
            json!({ "name": "alice", "password": "correct horse battery" }),
        ))
        .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.text());
    let token = login.json()["token"].as_str().unwrap().to_owned();
    let cookie = format!("gproxy_session={token}");

    let body = json!({ "name": "alice-key" });

    // A cookie POST caused by a foreign page.
    let foreign = with(
        with(post("/portal/api/keys", body.clone()), "cookie", &cookie),
        "origin",
        "https://evil.example",
    );
    let answer = host.send(foreign).await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.text());
    assert!(
        answer.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("cross-origin"),
        "{}",
        answer.text()
    );

    // The same request from the configured console origin.
    let allowed = with(
        with(post("/portal/api/keys", body.clone()), "cookie", &cookie),
        "origin",
        "https://console.example.com",
    );
    let answer = host.send(allowed).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());

    // And a key caller with no `Origin` at all is unaffected: a foreign page
    // cannot put a bearer token in a header, so the check would only break
    // every non-browser client.
    let answer = host
        .send(keyed(post("/portal/api/keys", body), "k-alice"))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
}

// ------------------------------------------------------------- /portal/api --

#[tokio::test]
async fn a_portal_login_lists_keys_and_logs_out() {
    let host = instance().await;

    // A cross-site form can post `text/plain` whose body is valid JSON; that
    // is a sign-in the person never made, so only a JSON media type counts.
    let mut forged = post(
        "/portal/api/login",
        json!({ "name": "alice", "password": "correct horse battery" }),
    );
    forged
        .headers_mut()
        .insert("content-type", "text/plain".parse().unwrap());
    let forged = host.send(forged).await;
    assert_eq!(forged.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let login = host
        .send(post(
            "/portal/api/login",
            json!({ "name": "alice", "password": "correct horse battery" }),
        ))
        .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.text());
    let token = login.json()["token"].as_str().unwrap().to_owned();
    let set_cookie = login.header("set-cookie").unwrap().to_owned();
    assert!(set_cookie.starts_with("gproxy_session="), "{set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(
        !set_cookie.contains("Secure"),
        "a plain-HTTP instance must not hand out a cookie the browser drops: {set_cookie}"
    );
    let cookie = format!("gproxy_session={token}");

    // The keys the caller owns, through the cookie.
    let keys = host
        .send(with(get("/portal/api/keys"), "cookie", &cookie))
        .await;
    assert_eq!(keys.status, StatusCode::OK, "{}", keys.text());
    let listed = keys.json();
    let ids: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["k-alice"], "only the caller's own keys");

    // The context a portal renders itself from.
    let context = host
        .send(with(get("/portal/api/context"), "cookie", &cookie))
        .await;
    assert_eq!(context.status, StatusCode::OK, "{}", context.text());
    assert_eq!(context.json()["user"]["name"], "alice");
    assert_eq!(context.json()["user"]["hasPassword"], true);

    // Sign out, which clears the cookie and reports that a row went.
    let out = host
        .send(with(
            post("/portal/api/logout", json!({})),
            "cookie",
            &cookie,
        ))
        .await;
    assert_eq!(out.status, StatusCode::OK, "{}", out.text());
    assert_eq!(out.json()["endedSession"], true);
    assert!(out.header("set-cookie").unwrap().contains("Max-Age=0"));

    // The cookie no longer authenticates anything.
    let keys = host
        .send(with(get("/portal/api/keys"), "cookie", &cookie))
        .await;
    assert_eq!(keys.status, StatusCode::UNAUTHORIZED);

    // A logout with a token that matches nothing still clears the cookie
    // rather than trapping a browser with one it can never discard.
    let out = host
        .send(with(
            post("/portal/api/logout", json!({})),
            "cookie",
            &cookie,
        ))
        .await;
    assert_eq!(out.status, StatusCode::OK);
    assert_eq!(out.json()["endedSession"], false);
}

#[tokio::test]
async fn a_wrong_password_is_the_same_answer_as_an_unknown_account() {
    let host = instance().await;
    let wrong = host
        .send(post(
            "/portal/api/login",
            json!({ "name": "alice", "password": "nope" }),
        ))
        .await;
    let unknown = host
        .send(post(
            "/portal/api/login",
            json!({ "name": "nobody", "password": "nope" }),
        ))
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.json(), unknown.json());
}

#[tokio::test]
async fn the_portal_is_reachable_by_an_ordinary_account_and_the_operator_surface_is_not() {
    let host = instance().await;
    let answer = host
        .send(keyed(get("/portal/api/context"), "k-alice"))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json()["user"]["id"], "alice");

    let answer = host.send(keyed(get("/admin/api/users"), "k-alice")).await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_admin_session_route_reports_the_caller_and_ends_the_session() {
    let host = instance().await;
    let answer = host.send(keyed(get("/admin/api/session"), "k-root")).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json()["userId"], "root");
    assert_eq!(answer.json()["userRole"], "admin");
    assert_eq!(answer.json()["apiKeyId"], "k-root");

    let answer = host
        .send(keyed(
            request(Method::DELETE, "/admin/api/session"),
            "k-root",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(answer.header("set-cookie").unwrap().contains("Max-Age=0"));
}

#[tokio::test]
async fn a_user_write_does_not_quote_the_password_into_the_trail() {
    let host = instance().await;
    let created = host
        .send(keyed(
            post(
                "/admin/api/users",
                json!({ "name": "dave", "password": "hunter2-longer-secret" }),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    for row in audit_rows(&host).await {
        assert!(
            !row.detail.to_string().contains("hunter2-longer-secret"),
            "the trail quoted a password: {}",
            row.detail
        );
    }
    // And the DTO does not carry it back either.
    assert!(created.json().get("password").is_none());
    assert_eq!(created.json()["hasPassword"], true);
}

#[tokio::test]
async fn a_malformed_body_is_a_400_from_the_binding_not_a_500() {
    let host = instance().await;
    let request = http::Request::builder()
        .method(Method::POST)
        .uri("/admin/api/users")
        .header("authorization", "Bearer k-root")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{ not json"))
        .unwrap();
    let answer = host.send(request).await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.text());
}

/// The write DTO is the one the operations suite uses; naming it here keeps
/// this file honest about which shape the router accepts.
#[allow(dead_code)]
fn user_write() -> UserWrite {
    UserWrite {
        name: "carol".into(),
        ..UserWrite::default()
    }
}
