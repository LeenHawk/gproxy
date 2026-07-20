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

#[tokio::test]
async fn observability_page_mode_returns_common_envelope() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-page", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    for (path, page, page_size) in [
        ("/admin/usage?page=1", 1, 50),
        ("/admin/logs?page=2&page_size=10", 2, 10),
        ("/admin/audit?page=1&page_size=100", 1, 100),
    ] {
        let p = parts("GET", path, Some(&cookie), None);
        let resp = run(&state, &p, b"").await.expect("page response");
        let value = parse_json(&resp);
        assert!(value["items"].as_array().unwrap().is_empty(), "{path}");
        assert_eq!(value["pagination"]["page"], page, "{path}");
        assert_eq!(value["pagination"]["page_size"], page_size, "{path}");
        assert_eq!(value["pagination"]["total_items"], 0, "{path}");
        assert_eq!(value["pagination"]["total_pages"], 0, "{path}");
    }

    let p = parts(
        "GET",
        "/admin/usage?page_size=not-a-number",
        Some(&cookie),
        None,
    );
    let resp = run(&state, &p, b"").await.expect("legacy response");
    assert!(parse_json(&resp).as_array().is_some());
}

#[tokio::test]
async fn numeric_pagination_rejects_invalid_parameters() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-page-invalid", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    for path in [
        "/admin/usage?page=0",
        "/admin/usage?page=1&page_size=0",
        "/admin/logs?page=1&page_size=101",
        "/admin/audit?page=2&before_id=10",
        "/admin/usage?page=18446744073709551615&page_size=100",
    ] {
        let p = parts("GET", path, Some(&cookie), None);
        let err = run(&state, &p, b"").await.expect_err(path);
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST, "{path}");
    }
}

#[tokio::test]
async fn usage_empty_summary_ok() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-obs-summary", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    let p = parts("GET", "/admin/usage-summary", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("200");
    assert_eq!(resp.status, http::StatusCode::OK);
    let v = parse_json(&resp);
    assert_eq!(v["requests"], 0);
    assert_eq!(v["cache_creation_30m_tokens"], 0);
    assert_eq!(v["cost"], "0");
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

#[tokio::test]
async fn audit_page_filters_items_and_total() {
    use crate::store::persistence::records::AuditLogInput;

    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-audit-filter", true).await;
    let cookie = cookie_for(&state, admin_id).await;
    let matching = state
        .persistence
        .append_audit_log(AuditLogInput {
            actor_id: Some(admin_id),
            actor_name: Some("admin-audit-filter".into()),
            action: "PATCH".into(),
            target: "/admin/credentials/5".into(),
            status: 204,
            source_ip: Some("203.0.113.9".into()),
        })
        .await
        .unwrap();
    state
        .persistence
        .append_audit_log(AuditLogInput {
            actor_id: Some(admin_id),
            actor_name: Some("admin-audit-filter".into()),
            action: "POST".into(),
            target: "/admin/providers".into(),
            status: 200,
            source_ip: Some("198.51.100.4".into()),
        })
        .await
        .unwrap();

    let path = format!(
        "/admin/audit?page=1&page_size=10&at_from={0}&at_to={0}&actor_id={1}\
         &action=AT&target=credentials&status=204&source_ip=203.0.113.9",
        matching.at, admin_id
    );
    let resp = run(&state, &parts("GET", &path, Some(&cookie), None), b"")
        .await
        .expect("filtered audit page");
    let value = parse_json(&resp);
    assert_eq!(value["pagination"]["total_items"], 1);
    assert_eq!(value["items"].as_array().unwrap().len(), 1);
    assert_eq!(value["items"][0]["id"], matching.id);
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
        list.as_array().unwrap().iter().any(|r| r["id"] == rp_id),
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

#[tokio::test]
async fn credential_model_status_routes_are_separate_and_guarded() {
    use crate::store::persistence::records::{
        CredentialInput, CredentialModelStatusInput, CredentialStatusInput, ProviderInput,
    };

    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-model-status", true).await;
    let cookie = cookie_for(&state, admin_id).await;
    let provider = state
        .persistence
        .upsert_provider(ProviderInput {
            id: None,
            name: "model-status-provider".into(),
            channel: "openai".into(),
            label: None,
            settings_json: serde_json::json!({}),
            credential_strategy: "round-robin".into(),
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .unwrap();
    let credential = state
        .persistence
        .upsert_credential(CredentialInput {
            id: None,
            provider_id: provider.id,
            name: Some("model-status-credential".into()),
            kind: "api_key".into(),
            secret_json: serde_json::json!({"api_key": "test"}),
            weight: 1,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        })
        .await
        .unwrap();
    state
        .persistence
        .upsert_credential_status(CredentialStatusInput {
            id: None,
            credential_id: credential.id,
            channel: "openai".into(),
            health_kind: "recovered".into(),
            health_json: None,
            checked_at: Some(10),
            last_error: None,
        })
        .await
        .unwrap();
    state
        .persistence
        .upsert_credential_model_status(CredentialModelStatusInput {
            id: None,
            credential_id: credential.id,
            channel: "openai".into(),
            model_id: "gpt-test".into(),
            health_kind: "rate_limited".into(),
            health_json: Some(serde_json::json!({"open_until": 20})),
            checked_at: Some(10),
            last_error: Some("limited".into()),
        })
        .await
        .unwrap();

    for path in [
        "/admin/credential-model-statuses".to_string(),
        format!("/admin/credentials/{}/model-statuses", credential.id),
    ] {
        let resp = run(&state, &parts("GET", &path, Some(&cookie), None), b"")
            .await
            .expect("model statuses");
        let rows = parse_json(&resp);
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["model_id"], "gpt-test");
    }

    for path in [
        "/admin/credential-statuses".to_string(),
        format!("/admin/credentials/{}/status", credential.id),
    ] {
        let global = run(&state, &parts("GET", &path, Some(&cookie), None), b"")
            .await
            .expect("global statuses");
        let rows = parse_json(&global);
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert!(rows[0].get("model_id").is_none());
    }

    let error = run(
        &state,
        &parts("GET", "/admin/credential-model-statuses", None, None),
        b"",
    )
    .await
    .expect_err("admin guard");
    assert_eq!(error.status(), http::StatusCode::UNAUTHORIZED);

    let user_id = seed_user(&state, "model-status-user", false).await;
    let user_cookie = cookie_for(&state, user_id).await;
    let error = run(
        &state,
        &parts(
            "GET",
            "/admin/credential-model-statuses",
            Some(&user_cookie),
            None,
        ),
        b"",
    )
    .await
    .expect_err("non-admin guard");
    assert_eq!(error.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn credential_model_status_routes_advertise_read_only_methods() {
    let (state, _dir) = state_with(vec![]).await;

    for path in [
        "/admin/credential-model-statuses",
        "/admin/credentials/1/model-statuses",
    ] {
        let resp = run(&state, &parts("OPTIONS", path, None, None), b"")
            .await
            .expect("known route");
        assert_eq!(resp.status, http::StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(resp.headers[http::header::ALLOW], "GET,HEAD");
    }
}
