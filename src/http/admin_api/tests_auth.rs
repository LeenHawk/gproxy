// Integration tests for edge dispatcher login / logout (B6.3 Task 1).
//
// Uses the SAME harness (state_with, seed_user, cookie_for, parts, run)
// from tests.rs. Exercises the REAL login path (password::verify + session),
// not the mint-and-use shortcut that the earlier tests use.

// ── helpers local to this file ─────────────────────────────────────────────

/// Build a JSON login body for `POST /admin/login`.
fn login_body(username: &str, password: &str) -> Vec<u8> {
    serde_json::json!({ "username": username, "password": password })
        .to_string()
        .into_bytes()
}

/// Extract the `set-cookie` header value from a `Resp`.
fn get_set_cookie(resp: &super::Resp) -> Option<String> {
    resp.headers
        .iter()
        .find(|(n, _)| *n == http::header::SET_COOKIE)
        .and_then(|(_, v)| v.to_str().ok())
        .map(str::to_owned)
}

/// Run the dispatcher expecting `Some(result)`.
async fn run_login(
    state: &AppState,
    username: &str,
    password: &str,
) -> Result<super::Resp, crate::api::error::ApiError> {
    let p = parts("POST", "/admin/login", None, None);
    run(state, &p, &login_body(username, password)).await
}

// ── tests ──────────────────────────────────────────────────────────────────

/// Correct credentials → 200, Set-Cookie with `gproxy_session=`, body has
/// `{user:{is_admin:true}}`. The returned cookie is then usable for /admin/me.
#[tokio::test]
async fn login_correct_password_returns_200_and_usable_cookie() {
    unsafe { std::env::set_var("GPROXY_INSECURE_COOKIES", "1") };
    let (state, _dir) = state_with(vec![]).await;
    // seed_user hashes "secret" via password::hash in tests.rs
    let _admin_id = seed_user(&state, "loginadmin", true).await;

    let resp = run_login(&state, "loginadmin", "secret")
        .await
        .expect("200 ok");
    assert_eq!(resp.status, http::StatusCode::OK, "status should be 200");

    // Body must carry user identity with is_admin:true.
    let v = parse_json(&resp);
    assert_eq!(v["user"]["name"], "loginadmin");
    assert_eq!(v["user"]["is_admin"], true);

    // Set-Cookie must contain the session token.
    let set_cookie = get_set_cookie(&resp).expect("Set-Cookie header present");
    assert!(
        set_cookie.contains("gproxy_session="),
        "cookie should start with gproxy_session=, got: {set_cookie}"
    );

    // Extract `name=value` (first semicolon-separated part) to use as Cookie.
    let cookie_kv = set_cookie.split(';').next().unwrap().to_string();

    // Use the session cookie for /admin/me → 200.
    let me_parts = parts("GET", "/admin/me", Some(&cookie_kv), None);
    let me = run(&state, &me_parts, b"").await.expect("me ok");
    assert_eq!(me.status, http::StatusCode::OK);
    let me_v = parse_json(&me);
    assert_eq!(me_v["name"], "loginadmin");
    assert_eq!(me_v["is_admin"], true);
}

/// Wrong password → 401, no Set-Cookie header.
#[tokio::test]
async fn login_wrong_password_returns_401_no_cookie() {
    let (state, _dir) = state_with(vec![]).await;
    let _admin_id = seed_user(&state, "loginbad", true).await;

    let err = run_login(&state, "loginbad", "wrongpassword")
        .await
        .expect_err("should be 401");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

/// Unknown username → 401, no Set-Cookie.
#[tokio::test]
async fn login_unknown_user_returns_401() {
    let (state, _dir) = state_with(vec![]).await;

    let err = run_login(&state, "ghost", "doesnotmatter")
        .await
        .expect_err("should be 401");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

/// After MAX_LOGIN_FAILS (5) wrong attempts, the next attempt → 429 with
/// a `Retry-After` header. Even the CORRECT password is throttled.
#[tokio::test]
async fn login_throttle_after_max_fails_returns_429() {
    let (state, _dir) = state_with(vec![]).await;
    let _admin_id = seed_user(&state, "throttleuser", true).await;

    // 5 wrong attempts → 401 each.
    for i in 0..5 {
        let err = run_login(&state, "throttleuser", "wrongpw")
            .await
            .expect_err(&format!("attempt {i} should be 401"));
        assert_eq!(
            err.status(),
            http::StatusCode::UNAUTHORIZED,
            "attempt {i}: expected 401"
        );
    }

    // 6th attempt (wrong) → 429 Too Many Requests.
    let err = run_login(&state, "throttleuser", "wrongpw")
        .await
        .expect_err("should be 429");
    assert_eq!(
        err.status(),
        http::StatusCode::TOO_MANY_REQUESTS,
        "6th attempt should be throttled"
    );

    // Even the correct password is now throttled.
    let err = run_login(&state, "throttleuser", "secret")
        .await
        .expect_err("correct pw also throttled");
    assert_eq!(
        err.status(),
        http::StatusCode::TOO_MANY_REQUESTS,
        "correct pw during lockout should be 429"
    );
}

/// Logout → 204 + clearing Set-Cookie (Max-Age=0); subsequent /admin/me with
/// the old cookie → 401 (session revoked).
#[tokio::test]
async fn logout_returns_204_and_revokes_session() {
    unsafe { std::env::set_var("GPROXY_INSECURE_COOKIES", "1") };
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "logoutadmin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Confirm session works pre-logout.
    let me_p = parts("GET", "/admin/me", Some(&cookie), None);
    let me = run(&state, &me_p, b"").await.expect("pre-logout me ok");
    assert_eq!(me.status, http::StatusCode::OK);

    // POST /admin/logout with the session cookie.
    let logout_p = parts("POST", "/admin/logout", Some(&cookie), None);
    let resp = run(&state, &logout_p, b"").await.expect("logout ok");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // Set-Cookie should clear the session (Max-Age=0).
    let set_cookie = get_set_cookie(&resp).expect("Set-Cookie header on logout");
    assert!(
        set_cookie.contains("Max-Age=0"),
        "clearing cookie should have Max-Age=0, got: {set_cookie}"
    );
    assert!(
        set_cookie.contains("gproxy_session="),
        "cookie name should be gproxy_session, got: {set_cookie}"
    );

    // Old session cookie → 401 (revoked).
    let me_p2 = parts("GET", "/admin/me", Some(&cookie), None);
    let err = run(&state, &me_p2, b"").await.expect_err("revoked session");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

/// Logout with no cookie → 204, no panic (idempotent).
#[tokio::test]
async fn logout_without_cookie_is_204() {
    let (state, _dir) = state_with(vec![]).await;
    let p = parts("POST", "/admin/logout", None, None);
    let resp = run(&state, &p, b"").await.expect("logout no cookie ok");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);
}

/// Successful login writes a `login.success` audit row; failed login writes
/// `login.fail` without the password.
#[tokio::test]
async fn login_writes_audit_rows() {
    let (state, _dir) = state_with(vec![]).await;
    let _id = seed_user(&state, "auditloginuser", true).await;

    // Failed login.
    let _ = run_login(&state, "auditloginuser", "badpass").await;
    // Successful login.
    let _ = run_login(&state, "auditloginuser", "secret").await;

    let rows = state.persistence.list_audit_logs(100).await.unwrap();
    assert!(
        rows.iter()
            .any(|r| r.action == "login.fail" && r.target == "auditloginuser" && r.actor_id.is_none()),
        "expected login.fail audit row, got {rows:?}"
    );
    assert!(
        rows.iter().any(|r| {
            r.action == "login.success"
                && r.actor_name.as_deref() == Some("auditloginuser")
                && r.actor_id.is_some()
        }),
        "expected login.success audit row, got {rows:?}"
    );
}
