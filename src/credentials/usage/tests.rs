use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http::{Response, StatusCode};
use serde_json::json;

use crate::channel::{Channel, Disposition};
use crate::http::client::{ClientError, UpstreamClient};

use super::UsageError;
use super::attempt::fetch_with;

struct CannedUpstream {
    status: StatusCode,
    body: &'static [u8],
}

#[async_trait]
impl UpstreamClient for CannedUpstream {
    async fn send(&self, _req: http::Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        Ok(Response::builder()
            .status(self.status)
            .body(Bytes::from_static(self.body))
            .unwrap())
    }
}

fn claudecode() -> Arc<dyn Channel> {
    crate::channel::registry::ChannelRegistry::with_builtin()
        .get("claudecode")
        .expect("claudecode registered")
}

#[tokio::test]
async fn fetch_with_parses_real_channel_response() {
    let client: Arc<dyn UpstreamClient> = Arc::new(CannedUpstream {
        status: StatusCode::OK,
        body: br#"{"five_hour":{"utilization":27,"resets_at":"2026-06-12T16:20:00+00:00"},
                  "seven_day":{"utilization":95,"resets_at":"2026-06-16T08:00:00+00:00"}}"#,
    });
    let secret = json!({ "access_token": "tok" });
    let snap = fetch_with(&claudecode(), &secret, &json!({}), &client)
        .await
        .expect("snapshot");
    let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["five_hour", "seven_day"]);
    assert_eq!(snap.windows[1].used_percent, Some(95.0));
}

#[tokio::test]
async fn non_2xx_upstream_is_status_error() {
    let client: Arc<dyn UpstreamClient> = Arc::new(CannedUpstream {
        status: StatusCode::TOO_MANY_REQUESTS,
        body: b"{}",
    });
    let err = fetch_with(
        &claudecode(),
        &json!({ "access_token": "t" }),
        &json!({}),
        &client,
    )
    .await
    .unwrap_err();
    assert!(matches!(err.error, UsageError::Status(429)));
    assert_eq!(
        err.disposition,
        Some(Disposition::RateLimited { retry_after: None })
    );
}
