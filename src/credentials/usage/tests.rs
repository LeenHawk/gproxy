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
    let raw_client: Arc<dyn UpstreamClient> = Arc::new(CannedUpstream {
        status: StatusCode::OK,
        body: br#"{"five_hour":{"utilization":27,"resets_at":"2026-06-12T16:20:00+00:00"},
                  "seven_day":{"utilization":95,"resets_at":"2026-06-16T08:00:00+00:00"}}"#,
    });
    let db = crate::store::persistence::DbPersistence::connect("sqlite::memory:")
        .await
        .unwrap();
    let credential = crate::store::persistence::records::Credential {
        id: 7,
        provider_id: 9,
        name: None,
        kind: "oauth".into(),
        secret_json: json!({}),
        weight: 1,
        rpm_limit: None,
        tpm_limit: None,
        proxy_url: None,
        tls_fingerprint: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };
    let audit = crate::credentials::audit::UpstreamAuditSequence::new(
        "usage",
        true,
        &db,
        &credential,
        true,
        false,
    );
    let client = audit.wrap_client(raw_client);
    let secret = json!({ "access_token": "tok" });
    let snap = fetch_with(&claudecode(), &secret, &json!({}), &client)
        .await
        .expect("snapshot");
    audit.persist(None).await;
    let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["five_hour", "seven_day"]);
    assert_eq!(snap.windows[1].used_percent, Some(95.0));
    let rows = crate::store::persistence::PersistenceBackend::list_upstream_requests(
        &db,
        audit.request_id(),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].request_id.starts_with("usage:7:"));
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
