//! Native integration tests for the cross-target admin/portal [`dispatch`]er.
//!
//! These run on the native target (the module is `cfg(any(wasm32, test))`),
//! driving the SAME dispatcher the wasm edge calls — proving the routing,
//! guards, CSRF and serialization without a wasm runtime.

use std::sync::Arc;

use bytes::Bytes;
use http::header;
use http::request::Parts;

use super::dispatch;
use crate::app::AppState;
use crate::app::snapshot::ControlPlaneSnapshot;
use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use crate::http::client::{ClientError, RespStream, UpstreamClient};
use crate::store::persistence::FilePersistence;
use crate::store::persistence::records::{OrgInput, UserInput};

struct NoUpstream;
#[async_trait::async_trait]
impl UpstreamClient for NoUpstream {
    async fn send(&self, _req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        unreachable!("admin_api tests do not call upstream")
    }
    async fn send_streaming(
        &self,
        _req: http::Request<Bytes>,
    ) -> Result<(http::StatusCode, http::HeaderMap, RespStream), ClientError> {
        unreachable!("admin_api tests do not call upstream")
    }
}

/// Build an AppState on a tempdir file store. `cors_origins` is supplied so the
/// CSRF test can assert a cross-origin `Origin` is refused.
async fn state_with(cors_origins: Vec<String>) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
        FilePersistence::open(dir.path().to_path_buf())
            .await
            .expect("open"),
    );
    let snapshot = ControlPlaneSnapshot::build(persistence.as_ref(), 1)
        .await
        .expect("snapshot");
    let config = Arc::new(RuntimeConfig {
        host: "127.0.0.1".into(),
        port: 0,
        cache: CacheConfig::Memory,
        persistence: PersistenceConfig::File {
            data_dir: dir.path().to_path_buf(),
        },
        upstream: UpstreamConfig::from_proxy_url(None),
        instance_id: 0,
        max_attempts: crate::config::DEFAULT_MAX_ATTEMPTS,
        max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_repo: None,
        update_channel: "releases".to_string(),
        update_data_dir: dir.path().to_path_buf(),
        cors_origins,
    });
    let cache: Arc<dyn crate::store::cache::CacheBackend> =
        Arc::new(crate::store::cache::MemoryCache::new());
    let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot));
    let channels = Arc::new(crate::channel::registry::ChannelRegistry::with_builtin());
    let state = AppState::new(
        config,
        cache,
        persistence,
        Arc::new(NoUpstream),
        snapshot,
        channels,
        Arc::new(crate::crypto::NoopCipher),
    );
    (state, dir)
}

/// Seed a user (admin or not) into the state's persistence; returns its id.
async fn seed_user(state: &AppState, name: &str, is_admin: bool) -> i64 {
    let org = state
        .persistence
        .upsert_org(OrgInput {
            id: None,
            name: format!("org-{name}"),
            enabled: true,
            description: None,
        })
        .await
        .unwrap();
    state
        .persistence
        .upsert_user(UserInput {
            id: None,
            name: name.into(),
            org_id: org.id,
            team_id: None,
            password: Some(crate::crypto::password::hash("secret").unwrap()),
            enabled: true,
            is_admin,
        })
        .await
        .unwrap()
        .id
}

/// Mint a session for `user_id` and return the `gproxy_session=…` cookie value.
async fn cookie_for(state: &AppState, user_id: i64) -> String {
    let token = crate::admin::session::create(state.cache.as_ref(), user_id)
        .await
        .expect("session create");
    format!("{}={token}", crate::admin::session::cookie_name())
}

/// Build a `Parts` for a request, optionally with a session cookie and Origin.
fn parts(method: &str, uri: &str, cookie: Option<&str>, origin: Option<&str>) -> Parts {
    let mut b = http::Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    if let Some(o) = origin {
        b = b.header(header::ORIGIN, o);
    }
    b.body(()).unwrap().into_parts().0
}

/// Run the dispatcher and unwrap the `Some` (the path must be one we handle).
async fn run(
    state: &AppState,
    p: &Parts,
    body: &[u8],
) -> Result<super::Resp, crate::api::error::ApiError> {
    dispatch(state, p, &Bytes::copy_from_slice(body))
        .await
        .expect("dispatcher handles this path")
}

fn parse_json(resp: &super::Resp) -> serde_json::Value {
    serde_json::from_slice(&resp.body).expect("json body")
}

#[tokio::test]
async fn admin_me_with_cookie_ok_without_cookie_401() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // With admin cookie → 200, body carries id/name/is_admin.
    let p = parts("GET", "/admin/me", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("ok");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    assert_eq!(v["id"].as_i64().unwrap(), admin_id);
    assert_eq!(v["name"], "admin");
    assert_eq!(v["is_admin"], true);

    // No cookie → 401 Unauthorized.
    let p = parts("GET", "/admin/me", None, None);
    let err = run(&state, &p, b"").await.expect_err("unauthorized");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn orgs_crud_roundtrip() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // POST create → 200, capture id.
    let p = parts("POST", "/admin/orgs", Some(&cookie), None);
    let resp = run(
        &state,
        &p,
        br#"{"name":"acme","enabled":true,"description":null}"#,
    )
    .await
    .expect("created");
    assert_eq!(resp.status, http::StatusCode::OK);
    let id = parse_json(&resp)["id"].as_i64().unwrap();

    // GET by id → 200.
    let p = parts("GET", &format!("/admin/orgs/{id}"), Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("get");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert_eq!(parse_json(&resp)["name"], "acme");

    // GET list contains it.
    let p = parts("GET", "/admin/orgs", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("list");
    let list = parse_json(&resp);
    assert!(list.as_array().unwrap().iter().any(|o| o["id"] == id));

    // DELETE → 204.
    let p = parts("DELETE", &format!("/admin/orgs/{id}"), Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("delete");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // GET by id again → 404.
    let p = parts("GET", &format!("/admin/orgs/{id}"), Some(&cookie), None);
    let err = run(&state, &p, b"").await.expect_err("gone");
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_me_non_admin_ok_no_cookie_401() {
    let (state, _dir) = state_with(vec![]).await;
    let user_id = seed_user(&state, "bob", false).await;
    let cookie = cookie_for(&state, user_id).await;

    // Non-admin session → 200 with is_admin:false.
    let p = parts("GET", "/user/me", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("ok");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    assert_eq!(v["id"].as_i64().unwrap(), user_id);
    assert_eq!(v["is_admin"], false);
    // Org/team resolve to names; seed_user creates org "org-bob" with no team.
    assert_eq!(v["org_name"], "org-bob");
    assert!(v["team_id"].is_null());
    assert!(v["team_name"].is_null());

    // No cookie → 401.
    let p = parts("GET", "/user/me", None, None);
    let err = run(&state, &p, b"").await.expect_err("unauthorized");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn non_admin_cannot_reach_admin_me() {
    let (state, _dir) = state_with(vec![]).await;
    let user_id = seed_user(&state, "bob", false).await;
    let cookie = cookie_for(&state, user_id).await;

    // A non-admin's session cookie does not authenticate as admin → 401.
    let p = parts("GET", "/admin/me", Some(&cookie), None);
    let err = run(&state, &p, b"").await.expect_err("not admin");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cross_origin_post_refused_403() {
    // The request's own host is `gproxy.test`; the Origin header is a different
    // site not in cors_origins, so the CSRF guard refuses the cookie-auth POST.
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts(
        "POST",
        "https://gproxy.test/admin/orgs",
        Some(&cookie),
        Some("https://evil.example"),
    );
    let err = run(
        &state,
        &p,
        br#"{"name":"x","enabled":true,"description":null}"#,
    )
    .await
    .expect_err("csrf refused");
    assert_eq!(err.status(), http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_admin_path_falls_through() {
    let (state, _dir) = state_with(vec![]).await;
    // A path under /admin/ we don't handle → dispatcher returns None.
    let p = parts("GET", "/admin/nope", None, None);
    assert!(dispatch(&state, &p, &Bytes::new()).await.is_none());
}

// ── providers CRUD (edge_crud! exercise) ─────────────────────────────────────

/// Minimal provider JSON body; channel/credential_strategy must be non-empty
/// strings; settings_json can be an empty object.
fn provider_body(name: &str) -> Vec<u8> {
    serde_json::json!({
        "id": null,
        "name": name,
        "channel": "openai",
        "label": null,
        "settings_json": {},
        "credential_strategy": "round-robin",
        "proxy_url": null,
        "tls_fingerprint": null,
        "enabled": true,
    })
    .to_string()
    .into_bytes()
}

#[tokio::test]
async fn providers_crud_roundtrip() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-p", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // POST → 200, capture id.
    let p = parts("POST", "/admin/providers", Some(&cookie), None);
    let resp = run(&state, &p, &provider_body("acme-ai"))
        .await
        .expect("created");
    assert_eq!(resp.status, http::StatusCode::OK);
    let id = parse_json(&resp)["id"].as_i64().unwrap();

    // GET by id → 200, name matches.
    let p = parts(
        "GET",
        &format!("/admin/providers/{id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("get");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert_eq!(parse_json(&resp)["name"], "acme-ai");

    // GET list contains it.
    let p = parts("GET", "/admin/providers", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("list");
    let list = parse_json(&resp);
    assert!(list.as_array().unwrap().iter().any(|o| o["id"] == id));

    // DELETE → 204.
    let p = parts(
        "DELETE",
        &format!("/admin/providers/{id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("delete");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // GET by id again → 404.
    let p = parts(
        "GET",
        &format!("/admin/providers/{id}"),
        Some(&cookie),
        None,
    );
    let err = run(&state, &p, b"").await.expect_err("gone");
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn providers_duplicate_name_is_409() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-dup", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // First upsert (id=null → insert) succeeds.
    let p = parts("POST", "/admin/providers", Some(&cookie), None);
    run(&state, &p, &provider_body("dup-name"))
        .await
        .expect("first insert ok");

    // Second insert with same name and id=null → unique-name violation → 409.
    let p = parts("POST", "/admin/providers", Some(&cookie), None);
    let err = run(&state, &p, &provider_body("dup-name"))
        .await
        .expect_err("duplicate");
    assert_eq!(err.status(), http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn providers_bad_id_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-bad", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Non-numeric id segment → parse_i64 → 400 BadRequest.
    let p = parts("GET", "/admin/providers/abc", Some(&cookie), None);
    let err = run(&state, &p, b"").await.expect_err("bad id");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

// Nested CRUD tests + instance_settings tests live in a separate file to stay
// under the 500-line cap. `include!` keeps them in the same test module so
// they share all helpers defined above.
include!("tests_nested.rs");

// Authz + observability integration tests (B6.2).
include!("tests_authz_obs.rs");

// Auth (login / logout) integration tests (B6.3 Task 1).
include!("tests_auth.rs");

// Special admin CRUD (user-keys / users / credentials) integration tests (B6.3 Task 2).
include!("tests_special.rs");

// Portal /user/* (session-scoped) integration tests (B6.3 Task 3).
include!("tests_portal.rs");

// Login-flows (edge-safe) + explicit 501 degradations (B6.3 Task 4).
include!("tests_login_flows.rs");

// Edge mutation-audit parity (B6.4).
include!("tests_audit.rs");
