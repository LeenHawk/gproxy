use super::*;
use crate::channel::ChannelError;
use http::StatusCode;

struct StatusClient;

#[async_trait::async_trait]
impl UpstreamClient for StatusClient {
    async fn send(&self, _req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        Ok(http::Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Bytes::from_static(
                br#"{"error":"invalid_grant","access_token":"response-secret"}"#,
            ))
            .unwrap())
    }
}

#[tokio::test]
async fn multi_call_refresh_persists_one_redacted_row_per_call() {
    let db = crate::store::persistence::DbPersistence::connect("sqlite::memory:")
        .await
        .unwrap();
    let credential = Credential {
        id: 7,
        provider_id: 9,
        name: None,
        kind: "oauth".into(),
        secret_json: serde_json::json!({}),
        weight: 1,
        rpm_limit: None,
        tpm_limit: None,
        proxy_url: None,
        tls_fingerprint: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };
    let audit = UpstreamAuditSequence::new("refresh", true, &db, &credential, true, false);
    let client = audit.wrap_client(Arc::new(StatusClient));
    let request = http::Request::post("https://auth.example/token?code=secret")
        .header("authorization", "secret")
        .body(Bytes::from_static(
            b"grant_type=refresh_token&refresh_token=request-secret&scope=openid",
        ))
        .unwrap();

    client.send(request).await.unwrap();
    let retry = http::Request::post("https://auth.example/token?code=retry-secret")
        .header("authorization", "retry-secret")
        .body(Bytes::from_static(b"refresh_token=retry-secret"))
        .unwrap();
    client.send(retry).await.unwrap();
    let error = ChannelError::Transient(
        "invalid_grant at https://auth.example/token?refresh_token=secret".into(),
    );
    let error = error.to_string();
    audit.persist(Some(&error)).await;

    let rows = db.list_upstream_requests(&audit.request_id).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.request_id == audit.request_id));
    let row = &rows[0];
    assert_eq!(row.provider_id, Some(9));
    assert_eq!(row.credential_id, Some(7));
    assert_eq!(row.url, "https://auth.example/token?code=[REDACTED]");
    assert_eq!(row.method, "POST");
    assert_eq!(row.status, 400);
    assert_eq!(
        row.headers_json.as_ref().unwrap()["authorization"],
        "[REDACTED]"
    );
    assert_eq!(
        row.body.as_deref(),
        Some("grant_type=refresh_token&refresh_token=[REDACTED]&scope=openid")
    );
    let response: Value = serde_json::from_str(row.response_body.as_deref().unwrap()).unwrap();
    assert_eq!(response["error"], "invalid_grant");
    assert_eq!(response["access_token"], "[REDACTED]");
}
