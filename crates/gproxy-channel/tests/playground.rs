#![cfg(feature = "playground")]

mod support;

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, OutboundClient,
    channel::{ChannelError, CredentialContext, NoState, OperationContext},
    channels::playground::Playground,
};
use gproxy_protocol::{
    Dialect, HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const RESPONSE: &str = include_str!("fixtures/playground/text-response.json");
const REQUEST: &str = include_str!("fixtures/playground/text-request.json");

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
    fn generation(response: &str) -> Arc<Self> {
        Self::new([
            (StatusCode::OK,json!({"ok":true,"proof":"fixture-proof","headers":{"X-Goog-Ext-519733851-bin":"fixture-binding"}}).to_string()),
            (StatusCode::OK,response.to_owned()),
        ])
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
                serde_json::from_slice(&body).unwrap(),
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
async fn captured_stream_preserves_tail_signature_and_final_usage_with_byte_sized_chunks() {
    let client = Script::generation(RESPONSE);
    let config = json!({"proof_server_url":"http://signer.test","timezone":"Asia/Shanghai"});
    let secret = secret();
    let body = json!({"contents":[{"role":"user","parts":[{"text":"Reply with exactly GPROXY_CAPTURE_OK."}]}],
        "generationConfig":{"maxOutputTokens":32768,"temperature":1,"topP":0.95,"topK":64,"thinkingConfig":{"includeThoughts":true,"thinkingLevel":"HIGH"}},
        "tools":[{"googleSearch":{}}]});
    let mut ctx = context(&config, &secret, client.clone(), body);
    ctx.endpoint_override = Some("https://rpc.test/exact-method");
    let response = Playground.stream_generate_content(ctx).await.unwrap();
    assert_eq!(response.headers["content-type"], "text/event-stream");
    let sse = drain(response.body).await.unwrap();
    let frames = sse
        .split("\n\n")
        .filter_map(|frame| frame.strip_prefix("data: "))
        .map(|data| serde_json::from_str::<Value>(data).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 2);
    assert_eq!(
        frames[0]["candidates"][0]["content"]["parts"][0]["text"],
        "GPROXY_CAPTURE_OK"
    );
    assert_eq!(
        frames[1]["candidates"][0]["content"]["parts"][0]["thoughtSignature"],
        "Zml4dHVyZS10aG91Z2h0LXNpZ25hdHVyZQ=="
    );
    assert_eq!(frames[1]["candidates"][0]["finishReason"], "STOP");
    assert_eq!(frames[1]["usageMetadata"]["totalTokenCount"], 305);
    assert_eq!(frames[1]["usageMetadata"]["thoughtsTokenCount"], 68);
    let sent = client.requests.lock().unwrap();
    assert_eq!(sent[0].0, "http://signer.test/v1/proof/mint");
    assert_eq!(sent[0].2["prompt"], "Reply with exactly GPROXY_CAPTURE_OK.");
    assert!(sent[0].1.get("authorization").is_none());
    assert_eq!(sent[1].0, "https://rpc.test/exact-method");
    assert!(
        sent[1].1["authorization"]
            .to_str()
            .unwrap()
            .starts_with("SAPISIDHASH ")
    );
    assert_eq!(
        sent[1].1["cookie"],
        "SAPISID=fixture-signing; SID=fixture-session; __Secure-1PSID=fixture-http-only"
    );
    assert_eq!(sent[1].1["x-goog-ext-519733851-bin"], "fixture-binding");
    let captured: Value = serde_json::from_str(REQUEST).unwrap();
    for index in [0, 1, 4, 6, 13] {
        assert_eq!(sent[1].2[index], captured[index]);
    }
}

#[tokio::test]
async fn buffering_keeps_text_and_signature_and_never_sums_cumulative_usage() {
    let config = json!({"proof_server_url":"http://signer.test"});
    let secret = secret();
    let text = "你好🦀";
    let fixture = RESPONSE.replace("GPROXY_CAPTURE_OK", text);
    let client = Script::generation(&fixture);
    let response = Playground
        .generate_content(context(&config, &secret, client, json!({"contents":[]})))
        .await
        .unwrap();
    let body: Value = serde_json::from_str(&drain(response.body).await.unwrap()).unwrap();
    assert_eq!(body["candidates"][0]["content"]["parts"][0]["text"], text);
    assert!(body["candidates"][0]["content"]["parts"][1]["thoughtSignature"].is_string());
    assert_eq!(body["usageMetadata"]["totalTokenCount"], 305);
    let client = Script::generation(&fixture[..fixture.len() - 3]);
    let response = Playground
        .stream_generate_content(context(&config, &secret, client, json!({"contents":[]})))
        .await
        .unwrap();
    assert!(
        drain(response.body)
            .await
            .unwrap_err()
            .contains("truncated RPC stream")
    );
}

#[tokio::test]
async fn captured_function_declarations_and_history_use_current_part_fields() {
    let config = json!({"proof_server_url":"http://signer.test"});
    let secret = secret();
    let client = Script::generation(RESPONSE);
    let body = json!({"contents":[
        {"role":"model","parts":[{"functionCall":{"name":"gproxy_capture_sum","id":"call-1","args":{"a":2,"b":3}},"thoughtSignature":"c2ln"}]},
        {"role":"user","parts":[{"functionResponse":{"name":"gproxy_capture_sum","id":"call-1","response":{"sum":5}}}]}
    ],"tools":[{"functionDeclarations":[{"name":"gproxy_capture_sum","description":"Add two numbers for a protocol capture.","parameters":{"type":"OBJECT","properties":{"a":{"type":"NUMBER"},"b":{"type":"NUMBER"}},"required":["a","b"],"propertyOrdering":["a","b"]}}]}]});
    let response = Playground
        .generate_content(context(&config, &secret, client.clone(), body))
        .await
        .unwrap();
    drain(response.body).await.unwrap();
    let sent = client.requests.lock().unwrap();
    assert_eq!(
        sent[1].2[6],
        serde_json::from_str::<Value>(include_str!("fixtures/playground/function-tools.json"))
            .unwrap()
    );
    assert_eq!(sent[1].2[1][0][0][0][10][2], "call-1");
    assert_eq!(sent[1].2[1][0][0][0][14], "c2ln");
    assert_eq!(sent[1].2[1][1][0][0][11][2], "call-1");
    assert_eq!(
        sent[0].2["prompt"], " ",
        "function-only parts contribute empty digest segments"
    );
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
    let client = Script::new([(StatusCode::OK, "[1]".into())]);
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
    let result = Playground
        .generate_content(context(
            &config,
            &secret,
            Script::new([]),
            json!({"contents":[]}),
        ))
        .await;
    assert!(
        matches!(result, Err(ChannelError::InvalidConfig(_))),
        "no implicit public signer"
    );
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
