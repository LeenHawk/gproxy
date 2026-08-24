#[path = "boot/fixture.rs"]
mod fixture;
#[path = "boot/seed.rs"]
mod seed;

use rust_decimal::Decimal;
use serde_json::json;

#[tokio::test]
async fn boots_relays_settles_and_reconciles_quota() {
    let fixture = fixture::Fixture::start().await;
    let window_start = current_window_start(seed::QUOTA_WINDOW_SECONDS);
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
    let mut quota = fixture
        .app
        .quota_window(fixture.quota_id, window_start)
        .await
        .expect("read quota");
    let settled_window = current_window_start(seed::QUOTA_WINDOW_SECONDS);
    if quota.is_none() && settled_window != window_start {
        quota = fixture
            .app
            .quota_window(fixture.quota_id, settled_window)
            .await
            .expect("read crossed quota window");
    }
    let quota = quota.expect("quota window");
    assert_eq!(quota.used_tokens, 15);
    assert!(
        !fixture
            .app
            .admission_pending(&request_id)
            .await
            .expect("read admission")
    );
    fixture.shutdown().await;
}

fn current_window_start(window_seconds: u64) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("current time")
        .as_secs() as i64;
    let window = window_seconds as i64;
    now - now.rem_euclid(window)
}
