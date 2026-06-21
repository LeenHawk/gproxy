// Integration tests for edge dispatcher special admin CRUD:
// user-keys (gen+seal+plaintext-once+ownership),
// users (password hash+redact+keep-existing),
// credentials (seal+redact+provider-scope) — B6.3 Task 2.
//
// Uses the SAME harness (state_with, seed_user, cookie_for, parts, run, parse_json)
// from tests.rs and the AppState with NoopCipher (sealing is identity-ish but
// the code path is fully exercised).

// ── user-keys ─────────────────────────────────────────────────────────────────

/// Create → response has `api_key` (the bare `sk-...` plaintext) ONCE.
/// GET list → key present with `key_prefix`, NO `api_key` in any item.
#[tokio::test]
async fn user_keys_create_returns_plaintext_once_list_redacts() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-keys", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // POST create → 200, api_key present.
    let p = parts(
        "POST",
        &format!("/admin/users/{admin_id}/keys"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, br#"{"label":"test-key","enabled":true}"#)
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

    // GET list → items present, NO api_key field on any item.
    let p = parts(
        "GET",
        &format!("/admin/users/{admin_id}/keys"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("list");
    assert_eq!(resp.status, http::StatusCode::OK);
    let list = parse_json(&resp);
    let items = list.as_array().unwrap();
    assert!(!items.is_empty(), "list should have at least one key");
    for item in items {
        assert!(
            item.get("api_key").is_none() || item["api_key"].is_null(),
            "api_key must be absent from list items (redacted): {:?}",
            item
        );
        // key_prefix must be present
        assert!(
            item.get("key_prefix").is_some(),
            "key_prefix must be present in list items"
        );
    }

    // DELETE → 204.
    let p = parts(
        "DELETE",
        &format!("/admin/user-keys/{key_id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("delete");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);
}

/// `POST /admin/users/{user_id}/keys` with `body.api_key` set → 400.
#[tokio::test]
async fn user_keys_create_with_api_key_in_body_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-k400", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts(
        "POST",
        &format!("/admin/users/{admin_id}/keys"),
        Some(&cookie),
        None,
    );
    let body = serde_json::json!({
        "label": "bad",
        "enabled": true,
        "api_key": "sk-should-be-rejected"
    })
    .to_string()
    .into_bytes();
    let err = run(&state, &p, &body).await.expect_err("should be 400");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

/// Update (id present) with wrong user_id ownership → 404.
#[tokio::test]
async fn user_keys_update_wrong_user_ownership_is_404() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-kown", true).await;
    let other_id = seed_user(&state, "other-kown", false).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Create a key under admin_id.
    let p = parts(
        "POST",
        &format!("/admin/users/{admin_id}/keys"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, br#"{"label":"owned","enabled":true}"#)
        .await
        .expect("created");
    let key_id = parse_json(&resp)["id"].as_i64().unwrap();

    // Try to update it via other_id's URL — ownership mismatch → 404.
    let p = parts(
        "POST",
        &format!("/admin/users/{other_id}/keys"),
        Some(&cookie),
        None,
    );
    let body = serde_json::json!({
        "id": key_id,
        "label": "stolen",
        "enabled": true,
    })
    .to_string()
    .into_bytes();
    let err = run(&state, &p, &body).await.expect_err("should be 404");
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

// ── users ─────────────────────────────────────────────────────────────────────

/// Create a user with a password → 200; GET list → UserView has NO password/hash
/// field (has_password: bool instead); POST /admin/login with those creds → 200
/// (proves the password was hashed correctly and is verifiable).
#[tokio::test]
async fn users_create_with_password_hash_on_set_redact_on_read_login_works() {
    unsafe { std::env::set_var("GPROXY_INSECURE_COOKIES", "1") };
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-u", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Need an org for the new user.
    let org = state
        .persistence
        .upsert_org(crate::store::persistence::records::OrgInput {
            id: None,
            name: "test-org".into(),
            enabled: true,
            description: None,
        })
        .await
        .unwrap();

    // POST create new user with password "ValidPass123!".
    let body = serde_json::json!({
        "id": null,
        "name": "newuser",
        "org_id": org.id,
        "team_id": null,
        "password": "ValidPass123!",
        "enabled": true,
        "is_admin": false,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/users", Some(&cookie), None);
    let resp = run(&state, &p, &body).await.expect("created");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    // `password` field must NOT appear in the response (only has_password).
    assert!(v.get("password").is_none(), "password must be redacted");
    let has_password = v["has_password"].as_bool().unwrap_or(false);
    assert!(has_password, "has_password should be true");
    let new_user_id = v["id"].as_i64().unwrap();

    // GET list — no `password` in any item.
    let p = parts("GET", "/admin/users", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("list");
    let list = parse_json(&resp);
    for item in list.as_array().unwrap() {
        assert!(
            item.get("password").is_none(),
            "password must not appear in user list: {:?}",
            item
        );
    }

    // GET by id — no `password`.
    let p = parts(
        "GET",
        &format!("/admin/users/{new_user_id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("get by id");
    let v = parse_json(&resp);
    assert!(v.get("password").is_none(), "password must be redacted in get");

    // POST /admin/login with the new user's credentials → 200 (hash was valid).
    let login_body = serde_json::json!({ "username": "newuser", "password": "ValidPass123!" })
        .to_string()
        .into_bytes();
    let p = parts("POST", "/admin/login", None, None);
    let resp = run(&state, &p, &login_body).await.expect("login ok");
    assert_eq!(
        resp.status,
        http::StatusCode::OK,
        "login must succeed proving password was hashed correctly"
    );
}

/// Update user with no password → existing hash kept (login still works after update).
#[tokio::test]
async fn users_update_without_password_keeps_existing_hash() {
    unsafe { std::env::set_var("GPROXY_INSECURE_COOKIES", "1") };
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-upd", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Create user with password.
    let org = state
        .persistence
        .upsert_org(crate::store::persistence::records::OrgInput {
            id: None,
            name: "org-upd".into(),
            enabled: true,
            description: None,
        })
        .await
        .unwrap();
    let body = serde_json::json!({
        "id": null,
        "name": "updatepwuser",
        "org_id": org.id,
        "team_id": null,
        "password": "ValidPass123!",
        "enabled": true,
        "is_admin": false,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/users", Some(&cookie), None);
    let resp = run(&state, &p, &body).await.expect("created");
    let created_id = parse_json(&resp)["id"].as_i64().unwrap();

    // Update user WITHOUT password (should keep existing hash).
    let body = serde_json::json!({
        "id": created_id,
        "name": "updatepwuser",
        "org_id": org.id,
        "team_id": null,
        "enabled": true,
        "is_admin": false,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/users", Some(&cookie), None);
    let resp = run(&state, &p, &body).await.expect("updated");
    assert_eq!(resp.status, http::StatusCode::OK);

    // Login still works with original password → hash was preserved.
    let login_body = serde_json::json!({ "username": "updatepwuser", "password": "ValidPass123!" })
        .to_string()
        .into_bytes();
    let p = parts("POST", "/admin/login", None, None);
    let resp = run(&state, &p, &login_body)
        .await
        .expect("login after update ok");
    assert_eq!(
        resp.status,
        http::StatusCode::OK,
        "login must succeed after no-password update"
    );
}

/// Bad password (< 12 chars) → 400.
#[tokio::test]
async fn users_create_with_bad_password_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-badpw", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let org = state
        .persistence
        .upsert_org(crate::store::persistence::records::OrgInput {
            id: None,
            name: "org-badpw".into(),
            enabled: true,
            description: None,
        })
        .await
        .unwrap();
    let body = serde_json::json!({
        "id": null,
        "name": "badpwuser",
        "org_id": org.id,
        "team_id": null,
        "password": "short",
        "enabled": true,
        "is_admin": false,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/users", Some(&cookie), None);
    let err = run(&state, &p, &body).await.expect_err("should be 400");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

/// DELETE /admin/users/{id} → 204.
#[tokio::test]
async fn users_delete_returns_204() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-del", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let org = state
        .persistence
        .upsert_org(crate::store::persistence::records::OrgInput {
            id: None,
            name: "org-del".into(),
            enabled: true,
            description: None,
        })
        .await
        .unwrap();
    let body = serde_json::json!({
        "id": null,
        "name": "deluser",
        "org_id": org.id,
        "team_id": null,
        "enabled": true,
        "is_admin": false,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/users", Some(&cookie), None);
    let resp = run(&state, &p, &body).await.expect("created");
    let del_id = parse_json(&resp)["id"].as_i64().unwrap();

    let p = parts(
        "DELETE",
        &format!("/admin/users/{del_id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("delete");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // GET by id → 404.
    let p = parts(
        "GET",
        &format!("/admin/users/{del_id}"),
        Some(&cookie),
        None,
    );
    let err = run(&state, &p, b"").await.expect_err("gone");
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

// ── credentials ───────────────────────────────────────────────────────────────

/// Create credential under a provider with a secret → 200;
/// GET list → secret is REDACTED (has_secret: true, no plaintext).
/// Provider-scope: GET credential with wrong provider_id → 404.
#[tokio::test]
async fn credentials_create_seals_and_list_redacts_fk_scope() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-cred", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Create a provider first.
    let prov_body = serde_json::json!({
        "id": null,
        "name": "cred-test-provider",
        "channel": "openai",
        "label": null,
        "settings_json": {},
        "credential_strategy": "round-robin",
        "proxy_url": null,
        "tls_fingerprint": null,
        "enabled": true,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/providers", Some(&cookie), None);
    let resp = run(&state, &p, &prov_body).await.expect("provider created");
    let provider_id = parse_json(&resp)["id"].as_i64().unwrap();

    // POST credential under the provider.
    let cred_body = serde_json::json!({
        "id": null,
        "label": "test-cred",
        "kind": "api_key",
        "secret_json": "sk-supersecret",
        "weight": 100,
        "enabled": true,
    })
    .to_string()
    .into_bytes();
    let p = parts(
        "POST",
        &format!("/admin/providers/{provider_id}/credentials"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, &cred_body).await.expect("cred created");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    // secret_json must NOT appear (redacted); has_secret must be true.
    assert!(
        v.get("secret_json").is_none(),
        "secret_json must be redacted in response: {:?}",
        v
    );
    assert!(
        v["has_secret"].as_bool().unwrap_or(false),
        "has_secret must be true"
    );
    let cred_id = v["id"].as_i64().unwrap();

    // GET list → each item has has_secret, no secret_json.
    let p = parts(
        "GET",
        &format!("/admin/providers/{provider_id}/credentials"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("list");
    let list = parse_json(&resp);
    for item in list.as_array().unwrap() {
        assert!(
            item.get("secret_json").is_none(),
            "secret_json must be absent from list: {:?}",
            item
        );
    }

    // GET credential by id with CORRECT provider_id → 200.
    let p = parts(
        "GET",
        &format!("/admin/providers/{provider_id}/credentials/{cred_id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("get by id");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    assert!(v.get("secret_json").is_none(), "secret_json redacted on get");

    // GET credential by id with WRONG provider_id → 404 (provider-scope check).
    let wrong_pid = provider_id + 9999;
    let p = parts(
        "GET",
        &format!("/admin/providers/{wrong_pid}/credentials/{cred_id}"),
        Some(&cookie),
        None,
    );
    let err = run(&state, &p, b"").await.expect_err("wrong provider → 404");
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);

    // DELETE → 204.
    let p = parts(
        "DELETE",
        &format!("/admin/credentials/{cred_id}"),
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("delete");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // GET by id after delete → 404.
    let p = parts(
        "GET",
        &format!("/admin/providers/{provider_id}/credentials/{cred_id}"),
        Some(&cookie),
        None,
    );
    let err = run(&state, &p, b"").await.expect_err("gone");
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

/// Create credential without secret_json on a new (id=null) record → 400.
#[tokio::test]
async fn credentials_create_without_secret_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-cs400", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Create provider.
    let prov_body = serde_json::json!({
        "id": null,
        "name": "cs400-provider",
        "channel": "openai",
        "label": null,
        "settings_json": {},
        "credential_strategy": "round-robin",
        "proxy_url": null,
        "tls_fingerprint": null,
        "enabled": true,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/providers", Some(&cookie), None);
    let resp = run(&state, &p, &prov_body).await.expect("provider");
    let provider_id = parse_json(&resp)["id"].as_i64().unwrap();

    // POST credential without secret_json (id=null, so this is a create).
    let cred_body = serde_json::json!({
        "id": null,
        "label": "no-secret",
        "kind": "api_key",
        "weight": 100,
        "enabled": true,
    })
    .to_string()
    .into_bytes();
    let p = parts(
        "POST",
        &format!("/admin/providers/{provider_id}/credentials"),
        Some(&cookie),
        None,
    );
    let err = run(&state, &p, &cred_body)
        .await
        .expect_err("should be 400");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}
