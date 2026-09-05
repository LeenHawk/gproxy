use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx, ResponseShapeCtx, UsageCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};

use super::ClineChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_truthful_operations() {
    let expected = [
        ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::OpenAi)),
        ChannelSupport::transform(
            family(Operation::ListModels, WireFamily::Claude),
            family(Operation::ListModels, WireFamily::OpenAi),
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
    ];
    assert_eq!(ClineChannel.descriptor().supports, expected);
}

#[test]
fn resolves_default_base_and_exact_override_urls() {
    let key = content(Operation::GenerateContent, Kind::OpenAiChat);
    let body = Bytes::from_static(br#"{"model":"route","messages":[]}"#);
    let defaults = json!({});
    let manual = prepare(
        key,
        "anthropic/claude",
        &body,
        &json!({"api_key":"manual"}),
        &defaults,
    );
    assert_eq!(
        manual.request.uri(),
        "https://api.cline.bot/api/v1/chat/completions"
    );
    assert_eq!(manual.request.method(), Method::POST);
    assert_eq!(manual.request.headers()["authorization"], "Bearer manual");
    assert_eq!(manual.request.headers()["accept"], "text/event-stream");

    let settings = json!({"base_url":"https://staging.cline.test/api/v1"});
    let account = prepare(
        key,
        "openai/gpt",
        &body,
        &json!({"access_token":"account-jwt","refresh_token":"refresh"}),
        &settings,
    );
    assert_eq!(
        account.request.uri(),
        "https://staging.cline.test/api/v1/chat/completions"
    );
    assert_eq!(
        account.request.headers()["authorization"],
        "Bearer workos:account-jwt"
    );

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"openai_list_models":"https://models.example/catalog"}
    });
    let list = prepare(
        family(Operation::ListModels, WireFamily::OpenAi),
        "",
        &Bytes::new(),
        &json!({"api_key":"manual"}),
        &settings,
    );
    assert_eq!(list.request.uri(), "https://models.example/catalog");
    assert_eq!(list.request.method(), Method::GET);
}

#[test]
fn shapes_catalog_and_generation_envelopes_without_defaults() {
    let list_key = family(Operation::ListModels, WireFamily::OpenAi);
    let raw = Bytes::from_static(
        br#"{"free":[{"id":"a/model"}],"clinePass":[{"id":"a/model"},{"id":"b/model"}]}"#,
    );
    let catalog = ClineChannel
        .shape_response(ResponseShapeCtx {
            key: list_key,
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &raw,
        })
        .unwrap();
    let catalog: Value = serde_json::from_slice(&catalog).unwrap();
    assert_eq!(catalog["data"].as_array().unwrap().len(), 2);
    assert_eq!(catalog["data"][0]["cline_group"], "free");
    assert!(catalog["data"][0].get("created").is_none());

    let key = content(Operation::GenerateContent, Kind::OpenAiChat);
    let raw = Bytes::from_static(
        br#"{"success":true,"data":{"id":"gen-1","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":4,"total_tokens":14}}}"#,
    );
    let headers = HeaderMap::new();
    let usage = ClineChannel
        .extract_usage(UsageCtx {
            key,
            request_body: &Bytes::new(),
            response_headers: &headers,
            response_body: &raw,
        })
        .unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (10, 4));
    let outward = ClineChannel
        .shape_response(ResponseShapeCtx {
            key,
            status: StatusCode::OK,
            headers: &headers,
            body: &raw,
        })
        .unwrap();
    let outward: Value = serde_json::from_slice(&outward).unwrap();
    assert_eq!(outward["id"], "gen-1");
    assert!(outward.get("data").is_none());
    assert!(outward.get("success").is_none());
}

fn prepare(
    key: OperationKey,
    model: &str,
    body: &Bytes,
    secret: &Value,
    settings: &Value,
) -> gproxy_channel_api::PreparedRequest {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    ClineChannel
        .prepare(PrepareCtx {
            session_id: None,
            key,
            stream: key.operation() == Operation::StreamGenerateContent,
            method: &Method::PATCH,
            path: "/client/path",
            query: Some("ignored=yes"),
            headers: &headers,
            body,
            upstream_model: model,
            provider_settings: settings,
            secret,
        })
        .unwrap()
}
