#![cfg(feature = "dashscope")]

//! DashScope puts four unrelated API layouts on one origin, and its image
//! API is not OpenAI-shaped in either direction.

use std::sync::Arc;

use gproxy_channel::channel::{PrepareContext, ResponseView, UsageContext, UsageExtractor};
use gproxy_channel::channels::dashscope::DashScope;
use gproxy_channel::{BaseChannel, ChannelBinding, ChannelError, LoginMode};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

mod support;
use support::{OneShot, credential, provider, request};

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let secret = json!({"api_key": "sk-dashscope"});
    DashScope.prepare(PrepareContext {
        provider: provider("dashscope", config, base_url),
        credential: credential("api_key", &secret, &Value::Null),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override: None,
    })
}

#[test]
fn each_operation_lands_on_the_prefix_that_serves_it() {
    for (operation, dialect, path, expected) in [
        (
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            "/v1/chat/completions",
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions",
        ),
        (
            Operation::GenerateContent,
            Dialect::Claude,
            "/v1/messages",
            "https://dashscope.aliyuncs.com/apps/anthropic/v1/messages",
        ),
        (
            Operation::ListModels,
            Dialect::OpenAi,
            "/v1/models",
            "https://dashscope.aliyuncs.com/compatible-mode/v1/models",
        ),
        (
            Operation::Rerank,
            Dialect::OpenAi,
            "/v1/rerank",
            "https://dashscope.aliyuncs.com/compatible-api/v1/reranks",
        ),
    ] {
        let prepared = prepare(&json!({}), None, operation, dialect, request(path, None)).unwrap();
        assert_eq!(prepared.uri(), expected, "{operation:?} {dialect:?}");
        assert_eq!(
            prepared.headers()["authorization"],
            "Bearer sk-dashscope",
            "every surface takes a bearer token, the Anthropic one included"
        );
        assert!(
            prepared.headers().get("anthropic-version").is_none(),
            "DashScope asks for none"
        );
    }
}

#[test]
fn an_image_request_becomes_the_native_envelope_at_the_native_path() {
    let mut caller = request("/v1/images/generations", None);
    caller.body = HttpBody::Bytes(Bytes::from(
        json!({"model": "qwen-image", "prompt": "a cat", "size": "1024x1024",
               "response_format": "url", "n": 1})
        .to_string(),
    ));
    let prepared = prepare(
        &json!({}),
        None,
        Operation::CreateImage,
        Dialect::OpenAi,
        caller,
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation",
        "images skip the compatibility prefixes entirely"
    );
    assert_eq!(prepared.headers()["content-type"], "application/json");
    let HttpBody::Bytes(bytes) = prepared.into_body() else {
        panic!("buffered");
    };
    let envelope: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        envelope["input"]["messages"][0]["content"],
        json!([{"text": "a cat"}])
    );
    assert_eq!(envelope["parameters"]["size"], "1024*1024");
    assert_eq!(envelope["parameters"]["watermark"], false);
    assert!(envelope["parameters"].get("response_format").is_none());
}

#[tokio::test]
async fn the_envelope_reply_is_folded_back_into_an_openai_image_reply() {
    let client = Arc::new(OneShot::new(
        StatusCode::OK,
        json!({"request_id": "r1", "output": {"choices": [{"message": {"content": [
            {"image": "https://cdn.example/1.png"}
        ]}}]}, "usage": {"image_count": 1, "input_tokens": 9}})
        .to_string(),
    ));
    let config = json!({});
    let secret = json!({"api_key": "sk-dashscope"});
    let mut caller = request("/v1/images/generations", None);
    caller.body = HttpBody::Bytes(Bytes::from(
        json!({"model": "qwen-image", "prompt": "a cat"}).to_string(),
    ));
    let response = ChannelBinding::new(
        &DashScope,
        provider("dashscope", &config, None),
        credential("api_key", &secret, &Value::Null),
        client.clone(),
    )
    .send(
        OperationKey {
            operation: Operation::CreateImage,
            dialect: Dialect::OpenAi,
        },
        caller,
    )
    .await
    .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert!(
        response.headers.get("content-length").is_none(),
        "the rewritten document has a length of its own"
    );
    let HttpBody::Bytes(bytes) = response.body else {
        panic!("buffered");
    };
    let reply: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reply["data"], json!([{"url": "https://cdn.example/1.png"}]));
    assert_eq!(reply["request_id"], "r1");
    assert!(
        reply.get("usage").is_none(),
        "DashScope's counters are not an OpenAI usage object"
    );
    assert_eq!(reply["dashscope_usage"]["image_count"], 1);

    // And that parked object is what the extractor reads.
    let text = bytes.to_vec();
    let headers = HeaderMap::new();
    let usage = DashScope
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::CreateImage,
                dialect: Dialect::OpenAi,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: &text,
            },
        })
        .unwrap()
        .unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(9));
    assert_eq!(usage.metrics.get("image_outputs"), Some(&1.into()));
}

#[tokio::test]
async fn an_upstream_error_on_the_image_path_is_returned_unchanged() {
    let client = Arc::new(OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"code": "InvalidParameter", "message": "no"}).to_string(),
    ));
    let config = json!({});
    let secret = json!({"api_key": "sk"});
    let mut caller = request("/v1/images/generations", None);
    caller.body = HttpBody::Bytes(Bytes::from(json!({"prompt": "x"}).to_string()));
    let response = ChannelBinding::new(
        &DashScope,
        provider("dashscope", &config, None),
        credential("api_key", &secret, &Value::Null),
        client,
    )
    .send(
        OperationKey {
            operation: Operation::CreateImage,
            dialect: Dialect::OpenAi,
        },
        caller,
    )
    .await
    .unwrap();
    assert_eq!(
        response.status,
        StatusCode::BAD_REQUEST,
        "a non-2xx is a response, not an error"
    );
    let HttpBody::Bytes(bytes) = response.body else {
        panic!("buffered");
    };
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["code"],
        "InvalidParameter"
    );
}

#[test]
fn an_image_request_that_cannot_be_converted_is_refused_before_the_call() {
    let mut streamed = request("/v1/images/edits", None);
    streamed.body = HttpBody::Stream(Box::pin(futures_util::stream::empty()));
    assert!(
        matches!(
            prepare(
                &json!({}),
                None,
                Operation::EditImage,
                Dialect::OpenAi,
                streamed
            ),
            Err(ChannelError::InvalidConfig(_))
        ),
        "a multipart upload is not converted"
    );

    let mut no_prompt = request("/v1/images/generations", None);
    no_prompt.body = HttpBody::Bytes(Bytes::from(json!({"model": "m"}).to_string()));
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            Operation::CreateImage,
            Dialect::OpenAi,
            no_prompt
        ),
        Err(ChannelError::InvalidConfig(_))
    ));
}

#[test]
fn the_descriptor_declares_three_native_shapes_and_no_account_surface() {
    let descriptor = DashScope.descriptor();
    assert_eq!(descriptor.id, "dashscope");
    assert_eq!(descriptor.login_modes, [LoginMode::ApiKey]);
    assert!(
        !descriptor.capabilities.quota_query,
        "the balance is a signed Alibaba Cloud RPC, not a DashScope endpoint"
    );
    assert_eq!(
        DashScope.native_dialects(
            provider("dashscope", &json!({}), None),
            Operation::GenerateContent
        ),
        [Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
    );
}
