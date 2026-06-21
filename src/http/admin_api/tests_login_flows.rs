// Integration tests for login-flows edge dispatcher + explicit 501 degradations
// (B6.3 Task 4).
//
// Uses the same harness (state_with, seed_user, cookie_for, parts, run,
// parse_json) from tests.rs.
//
// Coverage:
//   - /admin/login-flows/cookie → 501 (NotImplemented, type "not_implemented")
//   - /admin/update/check|status|apply → 501
//   - /admin/credentials/{id}/usage → 501
//   - /admin/login-flows/start without admin cookie → 401 (guard_admin runs first)
//   - /admin/login-flows/start with valid admin cookie + body → NOT 401/404; the
//     route is wired and guarded (codex channel authcode_start is client-free so
//     NoUpstream doesn't panic; we get 200 with a login_session_id).

// ── 501 degradations ─────────────────────────────────────────────────────────

/// `POST /admin/login-flows/cookie` → 501 with type "not_implemented".
#[tokio::test]
async fn login_flows_cookie_is_501() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "lf-cookie-admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts("POST", "/admin/login-flows/cookie", Some(&cookie), None);
    let err = run(&state, &p, br#"{"channel":"codex","cookie":"tok","provider_id":1}"#)
        .await
        .expect_err("501");
    assert_eq!(err.status(), http::StatusCode::NOT_IMPLEMENTED);
    assert_eq!(err.type_str(), "not_implemented");
}

/// `POST /admin/update/check` → 501.
#[tokio::test]
async fn update_check_is_501() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "lf-upd-admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    for endpoint in &["/admin/update/check", "/admin/update/status", "/admin/update/apply"] {
        let p = parts("POST", endpoint, Some(&cookie), None);
        let err = run(&state, &p, b"{}")
            .await
            .expect_err("501");
        assert_eq!(
            err.status(),
            http::StatusCode::NOT_IMPLEMENTED,
            "expected 501 for {endpoint}"
        );
        assert_eq!(err.type_str(), "not_implemented");
    }
}

/// `GET /admin/credentials/{id}/usage` → 501.
#[tokio::test]
async fn credential_usage_is_501() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "lf-cred-admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts("GET", "/admin/credentials/42/usage", Some(&cookie), None);
    let err = run(&state, &p, b"")
        .await
        .expect_err("501");
    assert_eq!(err.status(), http::StatusCode::NOT_IMPLEMENTED);
    assert_eq!(err.type_str(), "not_implemented");
}

// ── guard_admin on login-flows ────────────────────────────────────────────────

/// `POST /admin/login-flows/start` without any cookie → 401 (guard_admin fires).
#[tokio::test]
async fn login_flows_start_no_cookie_is_401() {
    let (state, _dir) = state_with(vec![]).await;

    let p = parts("POST", "/admin/login-flows/start", None, None);
    let err = run(&state, &p, br#"{"channel":"codex"}"#)
        .await
        .expect_err("401 unauthorized");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

/// `POST /admin/login-flows/start` with a non-admin session → 401.
#[tokio::test]
async fn login_flows_start_non_admin_is_401() {
    let (state, _dir) = state_with(vec![]).await;
    let user_id = seed_user(&state, "lf-plain-user", false).await;
    let cookie = cookie_for(&state, user_id).await;

    let p = parts("POST", "/admin/login-flows/start", Some(&cookie), None);
    let err = run(&state, &p, br#"{"channel":"codex"}"#)
        .await
        .expect_err("401 non-admin");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

/// `POST /admin/login-flows/start` with a valid admin cookie → the route is
/// wired and guarded: the codex channel's `authcode_start` is client-free, so
/// NoUpstream never panics, and we get 200 + a `login_session_id`.
#[tokio::test]
async fn login_flows_start_admin_ok_reaches_handler() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "lf-admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts("POST", "/admin/login-flows/start", Some(&cookie), None);
    let resp = run(&state, &p, br#"{"channel":"codex"}"#)
        .await
        .expect("must not be 401/404");
    // Guard passed, handler ran; codex authcode_start is pure → 200.
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    assert!(
        v["login_session_id"].as_str().is_some(),
        "expected login_session_id in response, got: {v}"
    );
    assert!(
        v["authorize_url"].as_str().is_some(),
        "expected authorize_url in response, got: {v}"
    );
}
