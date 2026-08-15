use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http::{HeaderMap, Method, Request, Response};
use serde_json::json;

use super::KimiCodeChannel;
use crate::channel::{Channel, ChannelLogin, DevicePoll, PrepareCtx};
use crate::http::client::{ClientError, UpstreamClient};
use crate::protocol::{ContentGenerationKind as Kind, Operation, OperationKey, OperationKind};
use crate::routing::RoutingDecision;

struct QueueUpstream {
    responses: Mutex<Vec<(u16, Bytes)>>,
    requests: Mutex<Vec<Request<Bytes>>>,
}

#[async_trait::async_trait]
impl UpstreamClient for QueueUpstream {
    async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        self.requests.lock().unwrap().push(request);
        let (status, body) = self.responses.lock().unwrap().remove(0);
        Ok(Response::builder().status(status).body(body).unwrap())
    }
}

fn client(responses: &[(u16, &'static [u8])]) -> Arc<QueueUpstream> {
    Arc::new(QueueUpstream {
        responses: Mutex::new(
            responses
                .iter()
                .map(|(status, body)| (*status, Bytes::from_static(body)))
                .collect(),
        ),
        requests: Mutex::new(Vec::new()),
    })
}

#[test]
fn prepares_managed_chat_request_with_complete_identity() {
    let secret = json!({
        "access_token": "oauth-token",
        "device_id": "device-1",
    });
    let settings = json!({});
    let headers = HeaderMap::new();
    let request = KimiCodeChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                Kind::OpenAiChatCompletions,
            ),
            stream: false,
            upstream_model_id: "kimi-for-coding",
            method: Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{}"),
        })
        .unwrap()
        .into_http()
        .unwrap();

    assert_eq!(
        request.uri(),
        "https://api.kimi.com/coding/v1/chat/completions"
    );
    assert_eq!(request.headers()["authorization"], "Bearer oauth-token");
    assert_eq!(request.headers()["user-agent"], "kimi-code-cli/0.36.1");
    assert_eq!(request.headers()["x-msh-platform"], "kimi_code_cli");
    assert_eq!(request.headers()["x-msh-version"], "0.36.1");
    assert_eq!(request.headers()["x-msh-device-id"], "device-1");
    for name in [
        "x-msh-device-name",
        "x-msh-device-model",
        "x-msh-os-version",
    ] {
        assert!(!request.headers()[name].is_empty());
    }
}

#[test]
fn converts_other_content_protocols_to_openai_chat() {
    let routes = KimiCodeChannel.routing_table();
    for kind in [
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
        Kind::GeminiGenerateContent,
    ] {
        let decision = routes
            .iter()
            .find(|(source, _)| {
                source.operation() == Operation::GenerateContent
                    && source.kind() == OperationKind::ContentGeneration(kind)
            })
            .map(|(_, decision)| *decision)
            .expect("missing Kimi Code route");
        assert!(matches!(
            decision,
            RoutingDecision::TransformTo(target)
                if target.kind()
                    == OperationKind::ContentGeneration(Kind::OpenAiChatCompletions)
        ));
    }
}

#[tokio::test]
async fn device_flow_reuses_one_identity_and_persists_it_in_secret() {
    let upstream = client(&[
        (
            200,
            br#"{"device_code":"upstream-device","user_code":"ABCD-EFGH","verification_uri":"https://www.kimi.com/device","verification_uri_complete":"https://www.kimi.com/device?code=ABCD-EFGH","interval":3}"#,
        ),
        (
            400,
            br#"{"error":"authorization_pending","error_description":"pending"}"#,
        ),
        (
            200,
            br#"{"access_token":"access","refresh_token":"refresh","expires_in":3600}"#,
        ),
    ]);
    let dynamic: Arc<dyn UpstreamClient> = upstream.clone();
    let settings = json!({});
    let init = KimiCodeChannel
        .device_start(
            &dynamic,
            crate::channel::DeviceStartCtx {
                provider_settings: &settings,
                params: &json!({}),
            },
        )
        .await
        .unwrap();
    assert_eq!(init.user_code, "ABCD-EFGH");
    assert_eq!(init.interval_secs, 3);
    assert!(init.device_code.starts_with("kimicode:"));

    assert!(matches!(
        KimiCodeChannel
            .device_poll(
                &dynamic,
                crate::channel::DevicePollCtx {
                    provider_settings: &settings,
                    device_code: &init.device_code,
                },
            )
            .await
            .unwrap(),
        DevicePoll::Pending
    ));
    let DevicePoll::Ready(secret) = KimiCodeChannel
        .device_poll(
            &dynamic,
            crate::channel::DevicePollCtx {
                provider_settings: &settings,
                device_code: &init.device_code,
            },
        )
        .await
        .unwrap()
    else {
        panic!("expected ready Kimi Code credential");
    };
    assert_eq!(secret["access_token"], "access");
    assert_eq!(secret["refresh_token"], "refresh");
    assert_eq!(secret["base_url"], "https://api.kimi.com/coding/v1");
    let device_id = secret["device_id"].as_str().unwrap();

    let requests = upstream.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    for request in requests.iter() {
        assert_eq!(request.headers()["x-msh-device-id"], device_id);
        assert_eq!(request.headers()["x-msh-platform"], "kimi_code_cli");
    }
    assert_eq!(
        String::from_utf8_lossy(requests[1].body()),
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=17e5f671-d194-4dfb-9706-5516cb48c098&device_code=upstream-device"
    );
}

#[tokio::test]
async fn refresh_rotates_both_tokens_with_the_persisted_device_identity() {
    let upstream = client(&[(
        200,
        br#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":7200}"#,
    )]);
    let dynamic: Arc<dyn UpstreamClient> = upstream.clone();
    let secret = json!({
        "access_token": "access-old",
        "refresh_token": "refresh-old",
        "expires_at_ms": 1,
        "device_id": "device-stable",
        "base_url": "https://api.kimi.com/coding/v1",
        "oauth_host": "https://auth.kimi.com"
    });
    assert!(KimiCodeChannel.needs_refresh(&secret));
    let refreshed = KimiCodeChannel
        .refresh(
            &dynamic,
            crate::channel::RefreshCtx {
                secret: &secret,
                provider_settings: &json!({}),
            },
        )
        .await
        .unwrap();
    assert_eq!(refreshed["access_token"], "access-new");
    assert_eq!(refreshed["refresh_token"], "refresh-new");
    assert_eq!(refreshed["device_id"], "device-stable");

    let requests = upstream.requests.lock().unwrap();
    assert_eq!(requests[0].headers()["x-msh-device-id"], "device-stable");
    assert_eq!(
        String::from_utf8_lossy(requests[0].body()),
        "client_id=17e5f671-d194-4dfb-9706-5516cb48c098&grant_type=refresh_token&refresh_token=refresh-old"
    );
}

#[test]
fn usage_request_targets_subscription_endpoint() {
    let secret = json!({
        "access_token": "oauth-token",
        "device_id": "device-1",
    });
    let request = KimiCodeChannel
        .prepare_usage_request(&secret, &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(request.method(), Method::GET);
    assert_eq!(request.uri(), "https://api.kimi.com/coding/v1/usages");
    assert_eq!(request.headers()["authorization"], "Bearer oauth-token");
    assert_eq!(request.headers()["x-msh-device-id"], "device-1");
}
