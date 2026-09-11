#[path = "boot/fixture.rs"]
mod fixture;
#[path = "boot/seed.rs"]
mod seed;

use rust_decimal::Decimal;
use serde_json::json;

#[tokio::test]
async fn admin_pages_fall_back_to_console_while_api_remains_namespaced() {
    let fixture = fixture::Fixture::start().await;
    let client = wreq::Client::builder().build().expect("downstream client");

    let page = client
        .get(fixture.url("/admin/providers"))
        .send()
        .await
        .expect("console page request");
    assert_eq!(page.status(), http::StatusCode::OK);
    assert_eq!(
        page.headers().get(http::header::CONTENT_TYPE),
        Some(&http::HeaderValue::from_static("text/html"))
    );
    let document = page.text().await.expect("console document");
    assert!(document.contains("<div id=\"root\"></div>"));

    let api = client
        .get(fixture.url("/admin/api/session"))
        .send()
        .await
        .expect("admin API request");
    assert_eq!(api.status(), http::StatusCode::OK);
    assert_eq!(
        api.headers().get(http::header::CONTENT_TYPE),
        Some(&http::HeaderValue::from_static(
            "application/json; charset=utf-8"
        ))
    );
    let session: serde_json::Value =
        serde_json::from_slice(&api.bytes().await.expect("session response"))
            .expect("session JSON");
    assert_eq!(session["setup_required"], true);
    fixture.shutdown().await;
}

#[tokio::test]
async fn boots_relays_settles_and_reconciles_quota() {
    let fixture = fixture::Fixture::start().await;
    let response = wreq::Client::builder()
        .build()
        .expect("downstream client")
        .post(fixture.gateway_url())
        .bearer_auth(&fixture.client_key)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(
            json!({
                "model": "public-model",
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        )
        .send()
        .await
        .expect("gateway request");
    assert_eq!(response.status(), http::StatusCode::OK);
    let request_id = response
        .headers()
        .get("x-request-id")
        .expect("request id header")
        .to_str()
        .expect("request id text")
        .to_owned();
    let body: serde_json::Value =
        serde_json::from_slice(&response.bytes().await.expect("gateway response body"))
            .expect("gateway response json");
    assert_eq!(body["choices"][0]["message"]["content"], "booted");

    // Native hosts settle after the response leaves; wait for the row to land.
    for _ in 0..200 {
        if !fixture
            .app
            .admission_pending(&request_id)
            .await
            .expect("read admission")
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let usage = fixture
        .app
        .usage_by_request(&request_id)
        .await
        .expect("read usage")
        .expect("usage row");
    assert_eq!(usage.usage.input_tokens, 10);
    assert_eq!(usage.usage.output_tokens, 5);
    assert!(usage.usage.cost > Decimal::ZERO);
    let quota = fixture
        .app
        .quota_windows()
        .await
        .expect("read quota windows")
        .into_iter()
        .find(|window| {
            window.quota_id == fixture.quota_id
                && window.window_kind == gproxy_store::records::QuotaWindowKind::Daily
        })
        .expect("daily quota window");
    assert_eq!(quota.cost_used, rust_decimal::Decimal::new(2, 5));
    assert!(quota.reset_at.is_some());
    assert!(
        !fixture
            .app
            .admission_pending(&request_id)
            .await
            .expect("read admission")
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn cors_changes_apply_without_rebinding_the_listener() {
    let fixture = fixture::Fixture::start().await;
    let client = wreq::Client::builder().no_proxy().build().unwrap();
    let sdk_headers = "authorization,content-type,x-stainless-arch,x-stainless-lang,x-stainless-os,x-stainless-package-version,x-stainless-retry-count,x-stainless-runtime,x-stainless-runtime-version,x-stainless-timeout";
    let preflight = || {
        client
            .request(http::Method::OPTIONS, fixture.gateway_url())
            .header(http::header::ORIGIN, "https://example.test")
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", sdk_headers)
    };
    assert!(
        !preflight()
            .send()
            .await
            .unwrap()
            .headers()
            .contains_key("access-control-allow-origin")
    );
    fixture
        .app
        .mutate(gproxy_app::ControlMutation::Setting(
            gproxy_store::records::SettingInput {
                key: "cors_origins".into(),
                value: json!(["https://EXAMPLE.test:443/"]),
            },
        ))
        .await
        .unwrap();
    let response = preflight().send().await.unwrap();
    assert_eq!(response.status(), http::StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://example.test"
    );
    let allowed: Vec<_> = response.headers()["access-control-allow-headers"]
        .to_str()
        .unwrap()
        .split(',')
        .map(str::trim)
        .collect();
    for header in sdk_headers.split(',') {
        assert!(allowed.contains(&header), "missing CORS header: {header}");
    }
    assert!(
        response
            .headers()
            .get_all(http::header::VARY)
            .iter()
            .any(|value| value == "Access-Control-Request-Headers")
    );
    fixture
        .app
        .mutate(gproxy_app::ControlMutation::Setting(
            gproxy_store::records::SettingInput {
                key: "cors_origins".into(),
                value: json!([]),
            },
        ))
        .await
        .unwrap();
    assert!(
        !preflight()
            .send()
            .await
            .unwrap()
            .headers()
            .contains_key("access-control-allow-origin")
    );
    fixture.shutdown().await;
}
