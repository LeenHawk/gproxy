use bytes::Bytes;
use http::{Method, StatusCode};
use serde_json::{Value, json};

#[tokio::test]
async fn runtime_settings_persist_normalize_and_report_startup_overrides() {
    let directory = tempfile::tempdir().unwrap();
    let key = crate::secrets::random_api_key().unwrap();
    let password = crate::secrets::random_password().unwrap();
    let config = || super::test_config(directory.path(), crate::MasterKeyConfig::new(None));
    let app = crate::App::start(config().with_native_options(crate::config::NativeOptions {
        admin_password: Some(password),
        bootstrap_admin_api_key: Some(key.clone()),
        max_attempts: Some(2),
        upstream_proxy_url: Some("http://127.0.0.1:19001".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    let (_, mut settings) = request(&app, &key, Method::GET, Value::Null).await;
    assert_eq!(settings["max_attempts"], 6);
    assert_eq!(settings["runtime_status"]["effective"]["max_attempts"], 2);
    let mut updates = app.subscribe_runtime_settings();
    settings["cors_origins"] = json!(["https://EXAMPLE.test:443/", "https://example.test"]);
    settings["trusted_proxies"] = json!(["2001:0db8::1", "2001:db8::1"]);
    settings["proxy"] = json!("http://127.0.0.1:19002");
    settings["max_attempts"] = json!(4);
    settings["max_in_flight"] = json!(3);
    settings["log_level"] = json!("debug");
    settings["log_format"] = json!("json");
    settings["runtime_status"] = Value::Null;
    let (status, saved) = request(&app, &key, Method::PATCH, settings).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["cors_origins"], json!(["https://example.test"]));
    assert_eq!(saved["trusted_proxies"], json!(["2001:db8::1"]));
    assert_eq!(saved["proxy"], "http://127.0.0.1:19002");
    assert_eq!(
        saved["runtime_status"]["effective"]["proxy"],
        "http://127.0.0.1:19001"
    );
    assert_eq!(saved["runtime_status"]["effective"]["max_attempts"], 2);
    assert_eq!(
        saved["runtime_status"]["overrides"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(updates.has_changed().unwrap());
    assert_eq!(updates.borrow_and_update().effective.max_in_flight, 3);

    for (field, value) in [
        ("cors_origins", json!(["https://example.test/path"])),
        ("trusted_proxies", json!(["not-an-ip"])),
        ("max_in_flight", json!(0)),
        ("max_attempts", json!(0)),
    ] {
        let mut invalid = saved.clone();
        invalid[field] = value;
        assert_eq!(
            request(&app, &key, Method::PATCH, invalid).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let (_, current) = request(&app, &key, Method::GET, Value::Null).await;
    assert_eq!(current, saved);
    app.shutdown();
    drop(app);

    let restarted = crate::App::start(config()).await.unwrap();
    let (_, current) = request(&restarted, &key, Method::GET, Value::Null).await;
    assert_eq!(current["max_attempts"], 4);
    assert_eq!(current["runtime_status"]["effective"]["max_attempts"], 4);
    assert_eq!(
        current["runtime_status"]["effective"]["proxy"],
        "http://127.0.0.1:19002"
    );
    assert_eq!(current["runtime_status"]["effective"]["log_format"], "json");
    assert_eq!(current["runtime_status"]["overrides"], json!([]));
    restarted.shutdown();
}

async fn request(
    app: &crate::AppHandle,
    key: &str,
    method: Method,
    value: Value,
) -> (StatusCode, Value) {
    let request = http::Request::builder()
        .method(method)
        .uri("/admin/api/instance-settings")
        .header(http::header::AUTHORIZATION, format!("Bearer {key}"))
        .body(())
        .unwrap();
    let response = app
        .admin_dispatch(&request.into_parts().0, Bytes::from(value.to_string()))
        .await
        .unwrap();
    (
        response.status(),
        serde_json::from_slice(response.body()).unwrap(),
    )
}

#[tokio::test]
async fn saved_attempt_limit_changes_the_actual_route_budget() {
    use gproxy_core::ControlPlane;
    let fixture = super::setup::fixture().await;
    fixture
        .app
        .inner
        .host
        .services
        .store
        .update_route(
            fixture.route,
            &gproxy_store::records::RouteInput {
                strategy: Default::default(),
                name: "test-route".into(),
                max_attempts: 10,
                enabled: true,
            },
        )
        .await
        .unwrap();
    for limit in [3, 8] {
        super::setting(&fixture.app, "max_attempts", json!(limit)).await;
        let plan = fixture
            .app
            .inner
            .host
            .services
            .control
            .resolve(
                Some("public-model"),
                &gproxy_core::RoutingMode::Aggregated,
                None,
            )
            .unwrap();
        assert_eq!(plan.budget.max_attempts, limit);
    }
}
