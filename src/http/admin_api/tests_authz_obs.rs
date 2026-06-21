// ── Authz + observability integration tests (B6.2) ───────────────────────────
//
// Shared with the outer test module via `include!` in tests.rs, so all helpers
// (state_with, seed_user, cookie_for, parts, run, parse_json) are in scope.
// `OrgInput` and `UserInput` are also already in scope from tests.rs.

/// Helper: seed a user belonging to an existing org (no separate org creation).
async fn seed_user_in_org(state: &AppState, name: &str, org_id: i64, is_admin: bool) -> i64 {
    state
        .persistence
        .upsert_user(UserInput {
            id: None,
            name: name.into(),
            org_id,
            team_id: None,
            password: Some(crate::crypto::password::hash("secret").unwrap()),
            enabled: true,
            is_admin,
        })
        .await
        .unwrap()
        .id
}

// ── GET /admin/usage?limit=5 → 200 empty array ───────────────────────────────

#[tokio::test]
async fn usage_empty_list_ok() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-obs", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts("GET", "/admin/usage?limit=5", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    assert!(v.as_array().unwrap().is_empty());
}

// ── GET /admin/usage with bad query param → 400 ──────────────────────────────

#[tokio::test]
async fn usage_bad_query_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-obs2", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // `limit` must be a u64; passing a string triggers serde_urlencoded error → 400.
    let p = parts("GET", "/admin/usage?limit=notanumber", Some(&cookie), None);
    let err = run(&state, &p, b"").await.expect_err("400");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

// ── GET /admin/audit → 200 ────────────────────────────────────────────────────

#[tokio::test]
async fn audit_empty_list_ok() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-audit", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts("GET", "/admin/audit", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200");
    assert_eq!(resp.status, http::StatusCode::OK);
    // Body is a JSON array (empty on a fresh store).
    assert!(parse_json(&resp).as_array().is_some());
}

// ── GET /admin/route-permissions?scope=user&scope_id=1 → 200 empty ───────────

#[tokio::test]
async fn route_permissions_empty_list_ok() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-authz", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts(
        "GET",
        "/admin/route-permissions?scope=user&scope_id=1",
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("200");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert!(parse_json(&resp).as_array().unwrap().is_empty());
}

// ── GET /admin/quotas?scope=user&scope_id=999 → 404 ──────────────────────────

#[tokio::test]
async fn quotas_missing_scope_is_404() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-quota", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts(
        "GET",
        "/admin/quotas?scope=user&scope_id=999",
        Some(&cookie),
        None,
    );
    let err = run(&state, &p, b"").await.expect_err("404");
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

// ── POST /admin/route-permissions then GET shows it ──────────────────────────

#[tokio::test]
async fn route_permissions_upsert_and_list() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-rp2", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Need a real org+user to use as scope_id.
    let org = state
        .persistence
        .upsert_org(crate::store::persistence::records::OrgInput {
            id: None,
            name: "rp-org".into(),
            enabled: true,
            description: None,
        })
        .await
        .unwrap();
    let user_id = seed_user_in_org(&state, "rp-user", org.id, false).await;

    // POST → 200, capture id.
    let body = serde_json::json!({
        "id": null,
        "scope": "user",
        "scope_id": user_id,
        "route_pattern": "*"
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/route-permissions", Some(&cookie), None);
    let resp = run(&state, &p, &body).await.expect("created");
    assert_eq!(resp.status, http::StatusCode::OK);
    let rp_id = parse_json(&resp)["id"].as_i64().unwrap();
    assert_eq!(parse_json(&resp)["route_pattern"], "*");

    // GET list for that scope_id → contains the record.
    let url = format!("/admin/route-permissions?scope=user&scope_id={user_id}");
    let p = parts("GET", &url, Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("list");
    assert_eq!(resp.status, http::StatusCode::OK);
    let list = parse_json(&resp);
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == rp_id),
        "inserted record should appear in scope list"
    );

    // DELETE → 204.
    let p = parts(
        "DELETE",
        &format!("/admin/route-permissions/{rp_id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("delete");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // List again → empty.
    let p = parts("GET", &url, Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("list after delete");
    assert!(parse_json(&resp).as_array().unwrap().is_empty());
}

// ── GET /admin/credential-statuses → 200 empty ───────────────────────────────

#[tokio::test]
async fn credential_statuses_empty_ok() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-cs", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts("GET", "/admin/credential-statuses", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert!(parse_json(&resp).as_array().unwrap().is_empty());
}
