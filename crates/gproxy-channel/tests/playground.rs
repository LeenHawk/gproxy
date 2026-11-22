#![cfg(feature = "playground")]

mod support;

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, OutboundClient, channel::OperationContext, channels::playground::Playground,
};
use gproxy_protocol::{
    Dialect, HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
};
use gproxy_upstream::{CredentialContext, NoState, UpstreamKind};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

struct Script {
    replies: Mutex<VecDeque<(StatusCode, String)>>,
    requests: Mutex<Vec<(String, HeaderMap, Value)>>,
}

impl Script {
    fn new(replies: impl IntoIterator<Item = (StatusCode, String)>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().collect()),
            requests: Mutex::new(vec![]),
        })
    }
}

impl OutboundClient for Script {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let HttpBody::Bytes(body) = body else {
                panic!("RPC request must be buffered")
            };
            self.requests.lock().unwrap().push((
                parts.uri.to_string(),
                parts.headers,
                // The RotateCookies placeholder body is not JSON.
                serde_json::from_slice(&body).unwrap_or(Value::Null),
            ));
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected outbound call");
            let chunks = body
                .into_bytes()
                .into_iter()
                .map(|byte| Ok(Bytes::from(vec![byte])))
                .collect::<Vec<_>>();
            let mut headers = HeaderMap::new();
            if status.is_client_error() {
                headers.insert("retry-after", HeaderValue::from_static("7"));
                headers.insert(
                    "set-cookie",
                    HeaderValue::from_static("SIDCC=fixture-rotated; Secure; HttpOnly"),
                );
            }
            Ok(WireResponse {
                status,
                headers,
                body: HttpBody::Stream(Box::pin(futures_util::stream::iter(chunks))),
            })
        })
    }
}

fn secret() -> Value {
    json!({"cookies":[
        {"domain":".google.com","name":"SID","value":"fixture-session"},
        {"domain":".google.com","name":"SAPISID","value":"fixture-signing"},
        {"domain":".google.com","name":"__Secure-1PSID","value":"fixture-http-only","httpOnly":true},
        {"domain":".google.com","name":"__Secure-1PSIDTS","value":"fixture-rotating","httpOnly":true},
        {"domain":".google.com","name":"analytics","value":"unrelated"},
        {"domain":"evil.example","name":"SAPISID","value":"wrong-domain"}
    ]})
}

fn context<'a>(
    config: &'a Value,
    secret: &'a Value,
    client: Arc<Script>,
    body: Value,
) -> OperationContext<'a> {
    let mut request = support::request(
        "/v1beta/models/gemini-4-hydrogen:generateContent",
        Some("key=caller-key"),
    );
    request.headers.insert(
        "cookie",
        HeaderValue::from_static("caller-session=do-not-forward"),
    );
    request.body = HttpBody::Bytes(body.to_string().into());
    OperationContext {
        provider: support::provider("playground", config, None),
        credential: support::credential("cookie", secret, &Value::Null),
        dialect: Dialect::Gemini,
        request,
        client,
        state: Arc::new(NoState::default()),
        instance_id: "local".into(),
        endpoint_override: None,
    }
}

async fn drain(body: HttpBody) -> Result<String, String> {
    let mut output = vec![];
    match body {
        HttpBody::Bytes(bytes) => output.extend_from_slice(&bytes),
        HttpBody::Stream(mut stream) => {
            while let Some(chunk) = stream.next().await {
                output.extend_from_slice(&chunk.map_err(|error| error.to_string())?);
            }
        }
    }
    String::from_utf8(output).map_err(|error| error.to_string())
}

#[tokio::test]
async fn imported_cookies_are_validated_and_rpc_permission_errors_remain_responses() {
    let config = json!({});
    let secret = secret();
    Playground.validate_credential_secret(&secret).unwrap();
    assert!(
        Playground
            .validate_credential_secret(&json!({"cookie":"SAPISID=only-signing-cookie"}))
            .is_err()
    );
    assert!(
        Playground
            .validate_credential_secret(&json!({"cookie":"SID=x; SAPISID=y\r\nCookie: z"}))
            .is_err()
    );
    let client = Script::new([
        (StatusCode::OK, String::new()),
        (StatusCode::OK, "[1]".into()),
    ]);
    let update = Playground
        .credential_refresh()
        .unwrap()
        .refresh(CredentialContext {
            provider: support::provider("playground", &config, None),
            credential: support::credential("cookie", &secret, &Value::Null),
            client: client.as_ref(),
        })
        .await
        .unwrap();
    assert_eq!(update.metadata.unwrap()["plan"], "Pro");
    assert!(update.secret.get("cookies").is_none());
    assert!(
        !update.secret["cookie"]
            .as_str()
            .unwrap()
            .contains("wrong-domain")
    );
    let client = Script::new([(
        StatusCode::FORBIDDEN,
        include_str!("fixtures/playground/permission-error.txt").into(),
    )]);
    let response = Playground
        .list_models(context(&config, &secret, client, json!({})))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(response.headers["retry-after"], "7");
    assert!(response.headers.get("set-cookie").is_none());
    let body: Value = serde_json::from_str(&drain(response.body).await.unwrap()).unwrap();
    assert_eq!(body["error"]["status"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn captured_catalog_preserves_model_limits_without_advertising_unsupported_methods() {
    let config = json!({});
    let secret = secret();
    let client = Script::new([(
        StatusCode::OK,
        include_str!("fixtures/playground/models-response.json").into(),
    )]);
    let response = Playground
        .get_model(context(&config, &secret, client, json!({})))
        .await
        .unwrap();
    let body: Value = serde_json::from_str(&drain(response.body).await.unwrap()).unwrap();
    assert_eq!(body["name"], "models/gemini-4-hydrogen");
    assert_eq!(body["inputTokenLimit"], 1048576);
    assert_eq!(body["outputTokenLimit"], 65536);
    assert_eq!(
        body["supportedGenerationMethods"],
        json!(["generateContent", "countTokens"])
    );
}
