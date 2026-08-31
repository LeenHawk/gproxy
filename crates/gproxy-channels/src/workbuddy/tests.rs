use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx, ResponseShapeCtx, UsageCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::WorkBuddyChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_truthful_operations() {
    let expected = [
        ChannelSupport::passthrough(family(Operation::ListModels)),
        ChannelSupport::transform(
            OperationKey::family(Operation::ListModels, WireFamily::Claude),
            family(Operation::ListModels),
        ),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiChat)),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::OpenAiResponses),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::ClaudeMessages),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::passthrough(content(Operation::StreamGenerateContent, Kind::OpenAiChat)),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::passthrough(family(Operation::CreateImage)),
        ChannelSupport::passthrough(family(Operation::EditImage)),
    ];
    assert_eq!(WorkBuddyChannel.descriptor().supports, expected);
}

#[test]
fn resolves_default_and_exact_override() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", "text/event-stream".parse().unwrap());
    let secret = json!({"access_token":"token","user_id":"user-1"});
    let default_settings = json!({});
    let list = WorkBuddyChannel
        .prepare(PrepareCtx {
            key: family(Operation::ListModels),
            stream: false,
            method: &Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: &Bytes::new(),
            upstream_model: "",
            provider_settings: &default_settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(list.request.uri(), "https://copilot.tencent.com/v3/config");

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"openai_chat_completions":"https://override.example/chat/{model}"}
    });
    let chat = WorkBuddyChannel
        .prepare(PrepareCtx {
            key: content(Operation::GenerateContent, Kind::OpenAiChat),
            stream: false,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[]}"#),
            upstream_model: "hunyuan/turbo",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        chat.request.uri(),
        "https://override.example/chat/hunyuan%2Fturbo"
    );
    assert_eq!(chat.request.headers()["authorization"], "Bearer token");
    assert_eq!(chat.request.headers()["accept"], "text/event-stream");
    let request_id = chat.request.headers()["x-request-id"].to_str().unwrap();
    assert_eq!(request_id.len(), 32);
    assert!(request_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let body: Value = serde_json::from_slice(chat.request.body()).unwrap();
    assert_eq!(body["model"], "hunyuan/turbo");
}

#[test]
fn converts_multipart_edits_without_fabricating_response_time() {
    let boundary = "workbuddy-boundary";
    let body = Bytes::from(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\ntrue\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"in.png\"\r\nContent-Type: image/png\r\n\r\nPNG\r\n--{boundary}--\r\n"
    ));
    let headers = HeaderMap::from_iter([(
        http::header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={boundary}")
            .parse()
            .unwrap(),
    )]);
    let settings = json!({});
    let secret = json!({"access_token":"token","user_id":"user-1"});
    let prepared = WorkBuddyChannel
        .prepare(PrepareCtx {
            key: family(Operation::EditImage),
            stream: false,
            method: &Method::POST,
            path: "/v1/images/edits",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "hunyuan-image-edit",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["model"], "hunyuan-image-edit");
    assert_eq!(shaped["prompt"], "true");
    assert_eq!(shaped["image"][0], "image/png;base64,UE5H");
    assert_eq!(shaped["response_format"], "b64_json");

    let streaming = WorkBuddyChannel.prepare(PrepareCtx {
        key: family(Operation::EditImage),
        stream: true,
        method: &Method::POST,
        path: "/v1/images/edits",
        query: None,
        headers: &headers,
        body: &body,
        upstream_model: "hunyuan-image-edit",
        provider_settings: &settings,
        secret: &secret,
    });
    assert!(streaming.is_err());

    let response = Bytes::from_static(
        br#"{"code":0,"requestId":"req-1","data":{"data":[{"b64_json":"abc"}]}}"#,
    );
    let outward = WorkBuddyChannel
        .shape_response(ResponseShapeCtx {
            key: family(Operation::EditImage),
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &response,
        })
        .unwrap();
    let outward: Value = serde_json::from_slice(&outward).unwrap();
    assert!(outward.get("created").is_none());
    assert_eq!(outward["data"][0]["b64_json"], "abc");
    assert_eq!(outward["requestId"], "req-1");
    assert_eq!(outward["code"], 0);

    let usage = WorkBuddyChannel
        .extract_usage(UsageCtx {
            key: family(Operation::EditImage),
            request_body: prepared.request.body(),
            response_headers: &HeaderMap::new(),
            response_body: &response,
        })
        .unwrap();
    assert_eq!(usage.metrics["image_outputs"], rust_decimal::Decimal::ONE);

    let error = Bytes::from_static(br#"{"code":17,"message":"denied"}"#);
    assert_eq!(
        WorkBuddyChannel
            .shape_response(ResponseShapeCtx {
                key: family(Operation::ListModels),
                status: StatusCode::OK,
                headers: &HeaderMap::new(),
                body: &error,
            })
            .unwrap(),
        error
    );
}
