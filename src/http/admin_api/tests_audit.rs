// Edge mutation-audit parity tests (B6.3 B6.4): the edge dispatcher audits
// non-GET admin/user mutations (method + path + actor + status), mirroring the
// native audit middleware. GETs and unauthenticated requests are NOT audited.

#[tokio::test]
async fn edge_mutation_writes_audit_row() {
    let (state, _dir) = state_with(vec![]).await;
    let admin_id = seed_user(&state, "admin", true).await;
    let cookie = cookie_for(&state, admin_id).await;

    // A mutating admin request (POST /admin/orgs) is audited.
    let p = parts("POST", "/admin/orgs", Some(&cookie), None);
    let resp = run(
        &state,
        &p,
        br#"{"name":"acme","enabled":true,"description":null}"#,
    )
    .await
    .expect("created");
    assert_eq!(resp.status, http::StatusCode::OK);

    let rows = state.persistence.list_audit_logs(50).await.expect("audit");
    let row = rows
        .iter()
        .find(|r| r.action == "POST" && r.target == "/admin/orgs")
        .expect("audit row for POST /admin/orgs");
    assert_eq!(row.actor_id, Some(admin_id));
    assert_eq!(row.status, 200);

    // A GET is NOT audited.
    let g = parts("GET", "/admin/orgs", Some(&cookie), None);
    let _ = run(&state, &g, b"").await;
    let rows = state.persistence.list_audit_logs(50).await.expect("audit");
    assert!(
        rows.iter().all(|r| r.action != "GET"),
        "GET requests must not be audited"
    );

    // An UNAUTHENTICATED mutation is NOT audited (no actor → like native, where
    // audit is inner to the auth guard and never runs for a 401).
    let np = parts("POST", "/admin/orgs", None, None);
    let _ = run(&state, &np, br#"{"name":"x","enabled":true,"description":null}"#).await;
    let rows = state.persistence.list_audit_logs(50).await.expect("audit");
    let post_orgs = rows
        .iter()
        .filter(|r| r.action == "POST" && r.target == "/admin/orgs")
        .count();
    assert_eq!(
        post_orgs, 1,
        "unauthenticated POST must not add an audit row"
    );
}

#[tokio::test]
async fn edge_portal_mutation_audited_as_session_user() {
    let (state, _dir) = state_with(vec![]).await;
    let uid = seed_user(&state, "alice", false).await;
    let cookie = cookie_for(&state, uid).await;

    // A portal key create (POST /user/keys) is audited with the session user as actor.
    let p = parts("POST", "/user/keys", Some(&cookie), None);
    let resp = run(&state, &p, br#"{"label":"laptop"}"#)
        .await
        .expect("created");
    assert_eq!(resp.status, http::StatusCode::OK);

    let rows = state.persistence.list_audit_logs(50).await.expect("audit");
    let row = rows
        .iter()
        .find(|r| r.action == "POST" && r.target == "/user/keys")
        .expect("audit row for POST /user/keys");
    assert_eq!(row.actor_id, Some(uid));
}
