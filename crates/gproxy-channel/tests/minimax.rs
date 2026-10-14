#![cfg(feature = "minimax")]

mod support;
use gproxy_channel::channel::{NoState, OperationContext, QuotaValue};
use gproxy_channel::channels::minimax::MiniMax;
use gproxy_channel::{BaseChannel, OutboundClient};
use gproxy_protocol::capability::{CapabilityError, CapabilityFuture};
use gproxy_protocol::{Dialect, HttpBody, WireResponse};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use support::{OneShot, credential, provider, request};

fn context<'a>(
    config: &'a Value,
    secret: &'a Value,
    path: &str,
    client: Arc<dyn OutboundClient>,
) -> OperationContext<'a> {
    OperationContext {
        provider: provider("minimax", config, None),
        credential: credential("api_key", secret, &Value::Null),
        dialect: Dialect::OpenAi,
        request: request(path, None),
        client,
        state: Arc::new(NoState::default()),
        instance_id: "test".into(),
        endpoint_override: None,
    }
}

fn value(response: WireResponse) -> Value {
    let HttpBody::Bytes(body) = response.body else {
        panic!("expected JSON")
    };
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn h3_create_maps_multimodal_input_and_returns_a_pending_job() {
    let config = json!({});
    let secret = json!({"api_key": "video-key"});
    let client = Arc::new(OneShot::new(
        StatusCode::OK,
        json!({"task_id":"1234"}).to_string(),
    ));
    let mut ctx = context(&config, &secret, "/v1/videos", client.clone());
    ctx.request.body = HttpBody::Bytes(json!({
        "model":"MiniMax-H3", "prompt":"A cat runs", "duration":5, "resolution":"2K", "aspect_ratio":"16:9",
        "input_references":[{"type":"image_url", "image_url":{"url":"https://example.com/cat.png"}}]
    }).to_string().into());
    let out = value(MiniMax.create_video(ctx).await.unwrap());
    assert_eq!(
        out,
        json!({"id":"1234", "status":"pending", "polling_url":"/v1/videos/1234"})
    );
    let (url, _, body) = client.call(0);
    assert_eq!(url, "https://api.minimax.io/v2/video_generation");
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["ratio"], "16:9");
    assert_eq!(
        body["content"],
        json!([
            {"type":"text", "text":"A cat runs"},
            {"type":"image_url", "image_url":{"url":"https://example.com/cat.png"}, "role":"reference_image"}
        ])
    );
    assert!(body.get("prompt").is_none());
}

struct DownloadClient {
    seen: Mutex<Vec<(String, HeaderMap)>>,
}

impl OutboundClient for DownloadClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let mut seen = self.seen.lock().unwrap();
            seen.push((request.uri().to_string(), request.headers().clone()));
            let body = if seen.len() == 1 {
                json!({"task":{"id":"1234", "status":"succeeded", "content":{"url":"https://cdn.example/video.mp4?signature=signed"}}}).to_string().into_bytes()
            } else {
                vec![0, 1, 2, 255]
            };
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(body.into()),
            })
        })
    }
}

#[tokio::test]
async fn video_download_uses_a_fresh_unauthenticated_cdn_request_and_preserves_bytes() {
    let config = json!({});
    let secret = json!({"api_key": "private-video-key"});
    let client = Arc::new(DownloadClient {
        seen: Mutex::new(vec![]),
    });
    let out = MiniMax
        .download_video_content(context(
            &config,
            &secret,
            "/v1/videos/1234/content",
            client.clone(),
        ))
        .await
        .unwrap();
    let HttpBody::Bytes(bytes) = out.body else {
        panic!("bytes")
    };
    assert_eq!(bytes.as_ref(), &[0, 1, 2, 255]);
    let seen = client.seen.lock().unwrap();
    assert_eq!(
        seen[0].0,
        "https://api.minimax.io/v2/query/video_generation/1234"
    );
    assert_eq!(seen[0].1["authorization"], "Bearer private-video-key");
    assert_eq!(seen[1].0, "https://cdn.example/video.mp4?signature=signed");
    assert!(seen[1].1.is_empty());
}

#[tokio::test]
async fn quota_distinguishes_remaining_from_used_counts_and_api_balance() {
    let payload = json!({"base_resp":{"status_code":0},"model_remains":[{
        "model_name":"MiniMax-M*", "current_interval_total_count":100,
        "current_interval_usage_count":20, "current_interval_remaining_percent":80,
        "end_time":1791000000000i64, "current_weekly_total_count":1000,
        "current_weekly_usage_count":900, "current_weekly_remaining_percent":90,
        "weekly_end_time":1791500000000i64
    }]})
    .to_string();
    let (url, snapshot) = support::quota(
        &MiniMax,
        "minimax",
        &json!({"api_key":"subscription-key"}),
        None,
        StatusCode::OK,
        payload,
    )
    .await
    .unwrap();
    assert_eq!(url, "https://api.minimax.io/v1/token_plan/remains");
    let QuotaValue::Window(interval) = &snapshot.entries[0].value else {
        panic!("window")
    };
    let QuotaValue::Window(weekly) = &snapshot.entries[1].value else {
        panic!("window")
    };
    assert_eq!(interval.remaining, Some(80.into()));
    assert_eq!(weekly.remaining, Some(900.into()));
    assert_eq!(weekly.period_end_ms, Some(1791500000000));
    let (url, snapshot) = support::quota(
        &MiniMax,
        "minimax",
        &json!({"api_key":"sk-api-account-key"}),
        None,
        StatusCode::OK,
        json!({"base_resp":{"status_code":0},"available_amount":"12.34"}).to_string(),
    )
    .await
    .unwrap();
    assert_eq!(url, "https://api.minimax.io/account/query_balance");
    let QuotaValue::Balance(balance) = &snapshot.entries[0].value else {
        panic!("balance")
    };
    assert_eq!(balance.remaining, Some("12.34".parse().unwrap()));
}
