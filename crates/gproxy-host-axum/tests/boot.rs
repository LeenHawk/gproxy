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
