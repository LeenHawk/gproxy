// Credential validation regression kept separate to enforce the file-size cap.

#[tokio::test]
async fn credentials_create_without_secret_is_400() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-cs400", true).await;
    let cookie = cookie_for(&state, admin_id).await;
    let provider = serde_json::json!({
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
    let request = parts("POST", "/admin/providers", Some(&cookie), None);
    let response = run(&state, &request, &provider).await.expect("provider");
    let provider_id = parse_json(&response)["id"].as_i64().unwrap();
    let credential = serde_json::json!({
        "id": null,
        "label": "no-secret",
        "kind": "api_key",
        "weight": 100,
        "enabled": true,
    })
    .to_string()
    .into_bytes();
    let request = parts(
        "POST",
        &format!("/admin/providers/{provider_id}/credentials"),
        Some(&cookie),
        None,
    );
    let error = run(&state, &request, &credential)
        .await
        .expect_err("secret is required");
    assert_eq!(error.status(), http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn credentials_import_reports_created_existing_error_per_item() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin-imp", true).await;
    let cookie = cookie_for(&state, admin_id).await;
    let provider = serde_json::json!({
        "id": null,
        "name": "imp-provider",
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
    let request = parts("POST", "/admin/providers", Some(&cookie), None);
    let response = run(&state, &request, &provider).await.expect("provider");
    let provider_id = parse_json(&response)["id"].as_i64().unwrap();

    // item0 creates; item1 is the same key → existing; item2 has no secret → error.
    let body = serde_json::json!({
        "items": [
            { "kind": "api_key", "secret_json": { "api_key": "sk-imp-1" }, "enabled": true },
            { "kind": "api_key", "secret_json": { "api_key": "sk-imp-1" }, "enabled": true },
            { "kind": "api_key", "enabled": true },
        ]
    })
    .to_string()
    .into_bytes();
    let request = parts(
        "POST",
        &format!("/admin/providers/{provider_id}/credentials/import"),
        Some(&cookie),
        None,
    );
    let response = run(&state, &request, &body).await.expect("import");
    let outcome = parse_json(&response);
    assert_eq!(outcome["created"], 1);
    assert_eq!(outcome["existing"], 1);
    assert_eq!(outcome["failed"], 1);
    assert_eq!(outcome["results"][0]["status"], "created");
    assert_eq!(outcome["results"][1]["status"], "existing");
    assert_eq!(
        outcome["results"][0]["id"], outcome["results"][1]["id"],
        "duplicate key resolves to the already-created credential"
    );
    assert_eq!(outcome["results"][2]["status"], "error");
}
