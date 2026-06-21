// Integration tests for the portal /user/* edge dispatcher (B6.3 Task 3).
//
// Uses the SAME harness (state_with, seed_user, cookie_for, parts, run, parse_json)
// from tests.rs. All tests use non-admin sessions except where noted.
//
// Coverage:
//   - /user/keys: create (plaintext-once), list (redacted), cross-user PATCH/DELETE → 404;
//     first user's key survives the cross-user attempt.
//   - /user/usage?user_id=<other>: still the session user's scope (field absent →
//     structurally un-smuggleable).
//   - /user/quota, /user/rate-limits, /user/route-permissions: effective rows
//     carry `source`; user sees only their own user-scope + their team/org.
//   - /user/change-password: wrong current → 400; weak new → 400; valid → 204;
//     old session cookie still works (not logged out); new password verifies via
//     POST /admin/login.
//   - guard_session: no cookie → 401 on every /user/* route.

// ── /user/keys ────────────────────────────────────────────────────────────────

/// `POST /user/keys` → response has `api_key` (plaintext-once); list → redacted;
/// a SECOND user's session cannot PATCH/DELETE the first user's key → 404; the
/// first user's key survives.
#[tokio::test]
async fn portal_keys_plaintext_once_and_cross_user_404() {
    let (state, _dir) = state_with(vec![]).await;
    let uid1 = seed_user(&state, "pk-user1", false).await;
    let uid2 = seed_user(&state, "pk-user2", false).await;
    let cookie1 = cookie_for(&state, uid1).await;
    let cookie2 = cookie_for(&state, uid2).await;

    // user1 creates a key → 200, api_key present.
    let p = parts("POST", "/user/keys", Some(&cookie1), None);
    let resp = run(&state, &p, br#"{"label":"my-key"}"#)
        .await
        .expect("created");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    let bare_key = v["api_key"]
        .as_str()
        .expect("api_key must be present on create (plaintext-once)");
    assert!(
        bare_key.starts_with("sk-"),
        "bare key should start with sk-, got: {bare_key}"
    );
    let key_id = v["id"].as_i64().unwrap();

    // user1 lists keys → api_key absent on list items, key_prefix present.
    let p = parts("GET", "/user/keys", Some(&cookie1), None);
    let resp = run(&state, &p, b"").await.expect("list");
    assert_eq!(resp.status, http::StatusCode::OK);
    let list = parse_json(&resp);
    let items = list.as_array().unwrap();
    assert!(!items.is_empty(), "list should not be empty");
    for item in items {
        assert!(
            item.get("api_key").is_none() || item["api_key"].is_null(),
            "api_key must be absent from list items: {:?}",
            item
        );
        assert!(
            item.get("key_prefix").is_some(),
            "key_prefix must be present in list items"
        );
    }

    // user2 attempts PATCH on user1's key → 404 (no existence leak).
    let p = parts(
        "PATCH",
        &format!("/user/keys/{key_id}"),
        Some(&cookie2),
        None,
    );
    let err = run(&state, &p, br#"{"label":"stolen","enabled":true}"#)
        .await
        .expect_err("cross-user PATCH must be 404");
    assert_eq!(
        err.status(),
        http::StatusCode::NOT_FOUND,
        "cross-user PATCH must return 404"
    );

    // user2 attempts DELETE on user1's key → 404.
    let p = parts(
        "DELETE",
        &format!("/user/keys/{key_id}"),
        Some(&cookie2),
        None,
    );
    let err = run(&state, &p, b"")
        .await
        .expect_err("cross-user DELETE must be 404");
    assert_eq!(
        err.status(),
        http::StatusCode::NOT_FOUND,
        "cross-user DELETE must return 404"
    );

    // user1's key still exists (cross-user attempt did NOT delete it).
    let p = parts("GET", "/user/keys", Some(&cookie1), None);
    let resp = run(&state, &p, b"").await.expect("list after cross-user");
    let list = parse_json(&resp);
    let survived = list
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k["id"] == key_id);
    assert!(survived, "user1's key must survive the cross-user attempt");
}

/// `POST /user/keys` with `api_key` in body → 400.
#[tokio::test]
async fn portal_keys_create_with_api_key_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "pk-400", false).await;
    let cookie = cookie_for(&state, uid).await;

    let p = parts("POST", "/user/keys", Some(&cookie), None);
    let err = run(
        &state,
        &p,
        br#"{"api_key":"sk-should-be-rejected","label":"bad"}"#,
    )
    .await
    .expect_err("should be 400");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

/// No cookie → 401 on `/user/keys` GET.
#[tokio::test]
async fn portal_keys_no_cookie_is_401() {
    let (state, _dir) = state_with(vec![]).await;
    let p = parts("GET", "/user/keys", None, None);
    let err = run(&state, &p, b"").await.expect_err("401");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

// ── /user/usage ───────────────────────────────────────────────────────────────

/// `GET /user/usage?user_id=<other>` — the `user_id` query param is silently
/// ignored (structurally absent from `MyUsageQuery`), so the response contains
/// only the session user's data.
#[tokio::test]
async fn portal_usage_user_id_param_is_ignored() {
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "pu-user", false).await;
    let other_id = seed_user(&state, "pu-other", false).await;
    let cookie = cookie_for(&state, uid).await;

    // Passing ?user_id=other_id must not cause a 400 (field is silently dropped)
    // and must return only the session user's records (empty here).
    let url = format!("/user/usage?user_id={other_id}");
    let p = parts("GET", &url, Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200 — param is dropped");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    assert!(
        v.as_array().unwrap().is_empty(),
        "session user has no usage; other user's data must not bleed through"
    );
}

/// No cookie → 401 on `/user/usage`.
#[tokio::test]
async fn portal_usage_no_cookie_is_401() {
    let (state, _dir) = state_with(vec![]).await;
    let p = parts("GET", "/user/usage", None, None);
    let err = run(&state, &p, b"").await.expect_err("401");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

// ── /user/quota|rate-limits|route-permissions ─────────────────────────────────

/// `GET /user/quota` with no records → empty array; once a user-scope quota is
/// upserted, the endpoint returns `[{source:"user", ...}]`.
#[tokio::test]
async fn portal_quota_effective_rows_have_source() {
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "pq-user", false).await;
    let cookie = cookie_for(&state, uid).await;
    let admin_id = seed_user(&state, "pq-admin", true).await;
    let admin_cookie = cookie_for(&state, admin_id).await;

    // Initially empty.
    let p = parts("GET", "/user/quota", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert!(
        parse_json(&resp).as_array().unwrap().is_empty(),
        "no quota yet"
    );

    // Upsert a user-scope quota via the admin endpoint.
    let body = serde_json::json!({
        "id": null,
        "scope": "user",
        "scope_id": uid,
        "window_seconds": 86400,
        "quota_total": "100.0",
        "cost_used": "0.0",
    })
    .to_string()
    .into_bytes();
    let p2 = parts("POST", "/admin/quotas", Some(&admin_cookie), None);
    run(&state, &p2, &body)
        .await
        .expect("quota upserted");

    // Now /user/quota should return one item with source:"user".
    let p = parts("GET", "/user/quota", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200");
    let list = parse_json(&resp);
    let items = list.as_array().unwrap();
    assert!(!items.is_empty(), "should have one quota row");
    assert_eq!(
        items[0]["source"], "user",
        "source must be 'user', got {:?}",
        items[0]
    );
}

/// No cookie → 401 on `/user/quota`.
#[tokio::test]
async fn portal_quota_no_cookie_is_401() {
    let (state, _dir) = state_with(vec![]).await;
    let p = parts("GET", "/user/quota", None, None);
    let err = run(&state, &p, b"").await.expect_err("401");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

/// `GET /user/rate-limits` → 200 empty; `GET /user/route-permissions` → 200 empty.
/// Guards that both endpoints are properly wired (smoke test).
#[tokio::test]
async fn portal_rate_limits_and_route_permissions_empty_ok() {
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "prl-user", false).await;
    let cookie = cookie_for(&state, uid).await;

    let p = parts("GET", "/user/rate-limits", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200 rate-limits");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert!(parse_json(&resp).as_array().unwrap().is_empty());

    let p = parts("GET", "/user/route-permissions", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200 route-permissions");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert!(parse_json(&resp).as_array().unwrap().is_empty());
}

// ── /user/change-password ─────────────────────────────────────────────────────

/// Wrong `current` password → 400; session cookie still valid.
#[tokio::test]
async fn portal_change_password_wrong_current_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "cpw-wrong", false).await;
    let cookie = cookie_for(&state, uid).await;

    let p = parts("POST", "/user/change-password", Some(&cookie), None);
    let body = serde_json::json!({ "current": "wrongpassword", "new": "NewValidPass123!" })
        .to_string()
        .into_bytes();
    let err = run(&state, &p, &body)
        .await
        .expect_err("wrong current → 400");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

/// Weak `new` password (< 12 chars) → 400.
#[tokio::test]
async fn portal_change_password_weak_new_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "cpw-weak", false).await;
    let cookie = cookie_for(&state, uid).await;

    let p = parts("POST", "/user/change-password", Some(&cookie), None);
    let body = serde_json::json!({ "current": "secret", "new": "short" })
        .to_string()
        .into_bytes();
    let err = run(&state, &p, &body)
        .await
        .expect_err("weak new → 400");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

/// Valid change: current correct, new meets policy → 204; old session cookie still
/// works (/user/me returns 200); new password verifies via POST /admin/login.
#[tokio::test]
async fn portal_change_password_valid_204_session_kept_new_pw_verifies() {
    unsafe { std::env::set_var("GPROXY_INSECURE_COOKIES", "1") };
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "cpw-ok", false).await;
    let cookie = cookie_for(&state, uid).await;

    // Change password: current is "secret" (seeded in seed_user).
    let p = parts("POST", "/user/change-password", Some(&cookie), None);
    let body = serde_json::json!({ "current": "secret", "new": "NewValidPass123!" })
        .to_string()
        .into_bytes();
    let resp = run(&state, &p, &body).await.expect("204 success");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // Old session cookie STILL works — session was not invalidated.
    let me_p = parts("GET", "/user/me", Some(&cookie), None);
    let me = run(&state, &me_p, b"").await.expect("/user/me after change");
    assert_eq!(me.status, http::StatusCode::OK);

    // New password must verify via POST /admin/login.
    let login_body = serde_json::json!({ "username": "cpw-ok", "password": "NewValidPass123!" })
        .to_string()
        .into_bytes();
    let login_p = parts("POST", "/admin/login", None, None);
    let resp = run(&state, &login_p, &login_body)
        .await
        .expect("login with new password");
    assert_eq!(
        resp.status,
        http::StatusCode::OK,
        "new password must verify via login"
    );

    // Old password must no longer work.
    let old_body = serde_json::json!({ "username": "cpw-ok", "password": "secret" })
        .to_string()
        .into_bytes();
    let login_p2 = parts("POST", "/admin/login", None, None);
    let err = run(&state, &login_p2, &old_body)
        .await
        .expect_err("old password rejected");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

/// No cookie → 401 on `/user/change-password`.
#[tokio::test]
async fn portal_change_password_no_cookie_is_401() {
    let (state, _dir) = state_with(vec![]).await;
    let p = parts("POST", "/user/change-password", None, None);
    let body = serde_json::json!({ "current": "secret", "new": "NewValidPass123!" })
        .to_string()
        .into_bytes();
    let err = run(&state, &p, &body).await.expect_err("401");
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}
