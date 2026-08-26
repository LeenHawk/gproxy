mod pressure;
mod quota;
mod setup;
mod tokenizer;

use bytes::Bytes;
use gproxy_core::CaptureSink;
use serde_json::json;

fn generation_operation() -> gproxy_protocol::OperationKey {
    gproxy_protocol::OperationKey::content(
        gproxy_protocol::Operation::GenerateContent,
        gproxy_protocol::ContentGenerationKind::OpenAiChat,
    )
}

#[tokio::test]
async fn log_sink_redacts_by_default_and_writes_clear_only_when_disabled() {
    let setup::Fixture {
        app,
        provider,
        credential,
        client_key,
        ..
    } = setup::fixture().await;
    for (key, value) in [
        (crate::cleanup::RETENTION_DAYS, json!(1)),
        (crate::logging::ENABLE_DOWNSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_DOWNSTREAM_LOG_BODY, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG_BODY, json!(true)),
    ] {
        setting(&app, key, value).await;
    }
    let mut request = setup::request("redacted", "hi", &client_key);
    request
        .headers
        .insert("x-api-key", "fixture-value".parse().unwrap());
    request.query = Some("token=fixture-value&mode=test".into());
    request.body = Bytes::from_static(br#"{"token":"fixture-value","ok":true}"#);
    crate::logging::begin(&app.inner.host, &request)
        .await
        .expect("downstream capture");
    CaptureSink::record(&app.inner.host, &capture(&request, provider, credential)).await;
    let detail = load_detail(&app, &request.request_id).await;
    let headers = detail.downstream.input.request_headers.as_ref().unwrap();
    assert_eq!(headers["authorization"], "[redacted]");
    assert_eq!(headers["x-api-key"], "[redacted]");
    assert_eq!(
        detail.downstream.input.query.as_deref(),
        Some("token=[redacted]&mode=test")
    );
    assert!(!body(&detail).contains("fixture-value"));

    setting(&app, crate::logging::DISABLE_LOG_REDACTION, json!(true)).await;
    let mut clear = request.clone();
    clear.request_id = "request-clear".into();
    crate::logging::begin(&app.inner.host, &clear)
        .await
        .expect("clear capture");
    CaptureSink::record(&app.inner.host, &capture(&clear, provider, credential)).await;
    let detail = load_detail(&app, &clear.request_id).await;
    let headers = detail.downstream.input.request_headers.as_ref().unwrap();
    assert_eq!(headers["authorization"], format!("Bearer {client_key}"));
    assert_eq!(headers["x-api-key"], "fixture-value");
    assert!(body(&detail).contains("fixture-value"));
}

#[tokio::test]
async fn upstream_metadata_is_recorded_without_bodies() {
    let setup::Fixture {
        app,
        provider,
        credential,
        client_key,
        ..
    } = setup::fixture().await;
    for (key, value) in [
        (crate::cleanup::RETENTION_DAYS, json!(1)),
        (crate::logging::ENABLE_DOWNSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG, json!(true)),
        (crate::logging::ENABLE_UPSTREAM_LOG_BODY, json!(false)),
    ] {
        setting(&app, key, value).await;
    }
    let request = setup::request("metadata", "hi", &client_key);
    crate::logging::begin(&app.inner.host, &request)
        .await
        .expect("downstream capture");
    CaptureSink::record(&app.inner.host, &capture(&request, provider, credential)).await;
    let detail = load_detail(&app, &request.request_id).await;
    let upstream = &detail.upstream[0].input;
    assert_eq!(upstream.request_method.as_deref(), Some("POST"));
    assert!(upstream.request_headers.is_some());
    assert_eq!(upstream.response_status, Some(200));
    assert!(upstream.response_headers.is_some());
    assert!(upstream.request_body.is_none());
    assert!(upstream.response_body.is_none());
}

fn capture(
    request: &gproxy_core::RequestCtx,
    provider: i64,
    credential: i64,
) -> gproxy_core::host::Capture {
    let mut response_headers = http::HeaderMap::new();
    response_headers.insert("set-cookie", "fixture-value".parse().unwrap());
    gproxy_core::host::Capture {
        request_id: request.request_id.clone(),
        provider_id: Some(provider),
        credential_id: Some(gproxy_core::CredentialId(credential)),
        upstream_url: Some("https://example.invalid/v1/test?token=fixture-value".into()),
        request_method: Some(http::Method::POST),
        request_headers: Some(request.headers.clone()),
        request_body: request.body.clone(),
        response_status: Some(http::StatusCode::OK),
        response_headers: Some(response_headers),
        response_body: Some(Bytes::from_static(br#"{"token":"fixture-value"}"#)),
    }
}

async fn setting(app: &crate::AppHandle, key: &str, value: serde_json::Value) {
    app.mutate(crate::ControlMutation::Setting(
        gproxy_store::records::SettingInput {
            key: key.into(),
            value,
        },
    ))
    .await
    .expect("log setting");
}

async fn load_detail(app: &crate::AppHandle, request_id: &str) -> gproxy_store::records::LogDetail {
    app.inner
        .host
        .services
        .store
        .log_detail(request_id)
        .await
        .unwrap()
        .expect("log detail")
}

fn body(detail: &gproxy_store::records::LogDetail) -> String {
    String::from_utf8(detail.upstream[0].input.request_body.clone().unwrap()).unwrap()
}
