// ── Nested CRUD tests (edge_crud_nested!) ─────────────────────────────────────

/// Nested round-trip: create provider → POST provider-model → list → DELETE → empty.
#[tokio::test]
async fn nested_provider_models_roundtrip() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-nested", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Create a provider to nest under.
    let p = parts("POST", "/admin/providers", Some(&cookie), None);
    let resp = run(&state, &p, &provider_body("nested-provider"))
        .await
        .expect("provider created");
    let pid = parse_json(&resp)["id"].as_i64().unwrap();

    // POST /admin/providers/{pid}/models → 200, capture model id.
    let url = format!("/admin/providers/{pid}/models");
    let model_body = serde_json::json!({
        "id": null, "provider_id": pid, "model_id": "gpt-4o",
        "display_name": null, "pricing_json": null, "variants_json": null, "enabled": true,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", &url, Some(&cookie), None);
    let resp = run(&state, &p, &model_body).await.expect("model created");
    assert_eq!(resp.status, http::StatusCode::OK);
    let mid = parse_json(&resp)["id"].as_i64().unwrap();
    assert_eq!(parse_json(&resp)["model_id"], "gpt-4o");

    // GET list → contains the model.
    let p = parts("GET", &url, Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("list");
    assert!(parse_json(&resp).as_array().unwrap().iter().any(|m| m["id"] == mid));

    // DELETE /admin/provider-models/{mid} → 204.
    let p = parts("DELETE", &format!("/admin/provider-models/{mid}"), Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("delete");
    assert_eq!(resp.status, http::StatusCode::NO_CONTENT);

    // GET list → now empty.
    let p = parts("GET", &url, Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("list after delete");
    assert!(parse_json(&resp).as_array().unwrap().is_empty());
}

/// fk-mismatch 400: POST /admin/orgs/{id}/teams with body org_id != URL org_id.
#[tokio::test]
async fn nested_team_fk_mismatch_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-fk", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Create an org to have a valid parent id.
    let p = parts("POST", "/admin/orgs", Some(&cookie), None);
    let resp = run(&state, &p, br#"{"name":"fk-org","enabled":true,"description":null}"#)
        .await
        .expect("org created");
    let org_id = parse_json(&resp)["id"].as_i64().unwrap();

    // Body carries a DIFFERENT org_id — must be rejected with 400.
    let wrong_body = serde_json::json!({
        "id": null, "org_id": org_id + 999, "name": "mismatch-team", "enabled": true,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", &format!("/admin/orgs/{org_id}/teams"), Some(&cookie), None);
    let err = run(&state, &p, &wrong_body).await.expect_err("fk mismatch");
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST, "fk mismatch must be 400");
}

/// instance_settings: GET (empty) → POST → GET (contains it).
#[tokio::test]
async fn instance_settings_get_post_roundtrip() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-is", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // Initially empty.
    let p = parts("GET", "/admin/instance-settings", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("get empty");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert!(parse_json(&resp).as_array().unwrap().is_empty(), "initially empty");

    // Upsert → 200, returns the record.
    let body = serde_json::json!({
        "id": null, "instance_name": "primary", "proxy": null, "spoof_emulation": null,
        "enable_usage": false, "enable_upstream_log": false, "enable_upstream_log_body": false,
        "enable_downstream_log": false, "enable_downstream_log_body": false,
        "disable_log_redaction": false, "enable_tokenizer_download": false,
        "update_channel": null, "retention_days": null,
    })
    .to_string()
    .into_bytes();
    let p = parts("POST", "/admin/instance-settings", Some(&cookie), None);
    let resp = run(&state, &p, &body).await.expect("upsert");
    assert_eq!(resp.status, http::StatusCode::OK);
    assert_eq!(parse_json(&resp)["instance_name"], "primary");

    // Now GET shows the record.
    let p = parts("GET", "/admin/instance-settings", Some(&cookie), None);
    let resp = run(&state, &p, b"").await.expect("get after upsert");
    let list = parse_json(&resp);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["instance_name"], "primary");
}
