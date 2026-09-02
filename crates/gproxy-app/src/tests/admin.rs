use bytes::Bytes;

#[tokio::test]
async fn explicit_environment_admin_password_updates_existing_account() {
    let directory = tempfile::tempdir().unwrap();
    let config = |password: &str| {
        super::test_config(directory.path(), crate::MasterKeyConfig::new(None)).with_native_options(
            crate::config::NativeOptions {
                admin_user: "operator".into(),
                admin_password: Some(password.into()),
                bootstrap_admin_api_key: Some("ordinary-admin-key".into()),
                ..Default::default()
            },
        )
    };
    let login = |password: &str| {
        let request = http::Request::post("/admin/api/login").body(()).unwrap();
        let (parts, _) = request.into_parts();
        let body = Bytes::from(
            serde_json::json!({"username": "operator", "password": password}).to_string(),
        );
        (parts, body)
    };

    let app = crate::App::start(config("first-password")).await.unwrap();
    let (parts, body) = login("first-password");
    assert_eq!(
        app.admin_dispatch(&parts, body).await.unwrap().status(),
        http::StatusCode::OK
    );
    let snapshot = app.inner.host.services.control.current();
    let admin = snapshot
        .users
        .iter()
        .find(|user| user.name == "operator")
        .expect("administrator is a user");
    assert!(admin.is_admin);
    assert!(admin.organization_id.is_some());
    assert!(snapshot.user_keys.iter().any(|key| key.user_id == admin.id));
    assert!(snapshot.permissions.iter().any(|permission| {
        permission.subject_kind == "user"
            && permission.subject_id == admin.id
            && permission.allowed
            && permission.provider_id.is_none()
            && permission.operation_group.is_none()
    }));
    let headers = http::HeaderMap::from_iter([(
        http::header::AUTHORIZATION,
        http::HeaderValue::from_static("Bearer ordinary-admin-key"),
    )]);
    assert_eq!(
        crate::host::authenticate_headers(&app.inner.host, &headers)
            .unwrap()
            .user_id,
        admin.id
    );
    let admin_request = http::Request::get("/admin/api/users")
        .header(http::header::AUTHORIZATION, "Bearer ordinary-admin-key")
        .body(())
        .unwrap();
    assert_eq!(
        app.admin_dispatch(&admin_request.into_parts().0, Bytes::new())
            .await
            .unwrap()
            .status(),
        http::StatusCode::OK
    );
    let portal_request = http::Request::get("/portal/api/context")
        .header(http::header::AUTHORIZATION, "Bearer ordinary-admin-key")
        .body(())
        .unwrap();
    assert_eq!(
        app.portal_dispatch(&portal_request.into_parts().0, Bytes::new())
            .await
            .unwrap()
            .status(),
        http::StatusCode::UNAUTHORIZED
    );
    drop(app);

    let app = crate::App::start(config("second-password")).await.unwrap();
    let (parts, body) = login("first-password");
    assert_eq!(
        app.admin_dispatch(&parts, body).await.unwrap().status(),
        http::StatusCode::UNAUTHORIZED
    );
    let (parts, body) = login("second-password");
    assert_eq!(
        app.admin_dispatch(&parts, body).await.unwrap().status(),
        http::StatusCode::OK
    );
}

#[tokio::test]
async fn mismatched_bootstrap_admin_does_not_prevent_restart() {
    let directory = tempfile::tempdir().unwrap();
    let config = |username: &str, password: &str| {
        super::test_config(directory.path(), crate::MasterKeyConfig::new(None)).with_native_options(
            crate::config::NativeOptions {
                admin_user: username.into(),
                admin_password: Some(password.into()),
                ..Default::default()
            },
        )
    };

    let app = crate::App::start(config("operator", "operator-password"))
        .await
        .unwrap();
    drop(app);

    let app = crate::App::start(config("admin", "stale-bootstrap-password"))
        .await
        .expect("an obsolete bootstrap username must not keep an existing store offline");
    let request = http::Request::post("/admin/api/login").body(()).unwrap();
    let body = Bytes::from(
        serde_json::json!({"username": "operator", "password": "operator-password"}).to_string(),
    );
    assert_eq!(
        app.admin_dispatch(&request.into_parts().0, body)
            .await
            .unwrap()
            .status(),
        http::StatusCode::OK
    );
}
