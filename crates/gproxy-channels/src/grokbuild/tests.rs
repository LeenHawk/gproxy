use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::GrokBuildChannel;

fn openai(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

struct Req<'a> {
    key: OperationKey,
    stream: bool,
    path: &'a str,
    body: &'a Bytes,
    model: &'a str,
    settings: &'a Value,
}

fn prepare(req: Req<'_>) -> http::Request<Bytes> {
    GrokBuildChannel
        .prepare(PrepareCtx {
            key: req.key,
            stream: req.stream,
            method: &Method::POST,
            path: req.path,
            query: None,
            headers: &HeaderMap::new(),
            body: req.body,
            upstream_model: req.model,
            provider_settings: req.settings,
            secret: &json!({"access_token":"oauth","refresh_token":"refresh","sub":"user-1"}),
        })
        .unwrap()
        .request
}

#[test]
fn descriptor_declares_exact_runtime_routes() {
    let descriptor = GrokBuildChannel.descriptor();
    assert_eq!(descriptor.id, "grokbuild");
    assert_eq!(descriptor.supports.len(), 21);
    assert_eq!(
        descriptor
            .supports
            .iter()
            .filter(|support| support.source == support.target)
            .count(),
        15
    );
    for source in [
        OperationKey::content(Operation::GenerateContent, Kind::ClaudeMessages),
        OperationKey::content(Operation::GenerateContent, Kind::GeminiGenerateContent),
    ] {
        assert!(descriptor.supports.iter().any(|support| {
            support.source == source
                && support.target
                    == OperationKey::content(
                        Operation::StreamGenerateContent,
                        Kind::OpenAiResponses,
                    )
        }));
    }
    assert!(
        !descriptor
            .supports
            .iter()
            .any(|support| { matches!(support.source.operation, Operation::CreateEmbedding) })
    );
    assert!(descriptor.supports.iter().any(|support| {
        support.source == openai(Operation::CompactContent) && support.source == support.target
    }));
}

#[test]
fn prepare_resolves_cli_and_media_default_and_override_urls() {
    let response_body = Bytes::from_static(br#"{"model":"route","input":"hi","stream":true}"#);
    let response = prepare(Req {
        key: OperationKey::content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        stream: true,
        path: "/v1/responses",
        body: &response_body,
        model: "grok-4.5",
        settings: &json!({}),
    });
    assert_eq!(
        response.uri(),
        "https://cli-chat-proxy.grok.com/v1/responses"
    );
    assert_eq!(response.headers()["authorization"], "Bearer oauth");
    assert_eq!(response.headers()["accept"], "text/event-stream");
    assert_eq!(response.headers()["x-xai-token-auth"], "xai-grok-cli");
    assert_eq!(response.headers()["x-grok-user-id"], "user-1");

    let image_body = Bytes::from_static(br#"{"model":"route","prompt":"cat"}"#);
    let image = prepare(Req {
        key: openai(Operation::CreateImage),
        stream: false,
        path: "/v1/images/generations",
        body: &image_body,
        model: "grok-imagine-image-quality",
        settings: &json!({
            "base_url":"https://unused.example",
            "endpoints":{"image_generations":"https://media.example/image"}
        }),
    });
    assert_eq!(image.uri(), "https://media.example/image");
}

#[test]
fn prepare_applies_responses_and_xai_media_hygiene() {
    let responses = Bytes::from(
        json!({
            "model":"route","stream":true,"metadata":{"private":true},
            "previous_response_id":"old","top_p":0,
            "include":["reasoning.encrypted_content"],
            "reasoning":{"effort":"high"},
            "input":[
                {"type":"reasoning","summary":[{"type":"summary_text","text":"a"}],
                    "encrypted_content":"gAAAAinvalid"},
                {"type":"reasoning","summary":[{"type":"summary_text","text":"b"}]},
                {"type":"compaction","encrypted_content":"bad"}
            ],
            "tools":[
                {"type":"custom","name":"shell"},
                {"type":"function","name":"plain"},
                {"type":"web_search_preview","search_context_size":"low"},
                {"type":"image_generation"}
            ],
            "tool_choice":{"type":"allowed_tools","tools":[{"type":"x_search"}]}
        })
        .to_string(),
    );
    let responses = prepare(Req {
        key: OperationKey::content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        stream: true,
        path: "/v1/responses",
        body: &responses,
        model: "grok-composer-fast",
        settings: &json!({}),
    });
    let session_header = responses.headers()["x-grok-conv-id"].clone();
    let responses: Value = serde_json::from_slice(responses.body()).unwrap();
    assert!(responses.get("metadata").is_none());
    assert!(responses.get("previous_response_id").is_none());
    assert!(responses.get("top_p").is_none());
    assert_eq!(responses["tools"][0]["type"], "function");
    assert_eq!(responses["tools"][0]["parameters"]["type"], "object");
    assert_eq!(responses["tools"][1]["parameters"]["type"], "object");
    assert_eq!(responses["tools"][2]["type"], "web_search");
    assert!(responses.get("tool_choice").is_none());
    assert!(responses.get("include").is_none());
    assert!(responses.get("reasoning").is_none());
    assert_eq!(responses["input"].as_array().unwrap().len(), 1);
    assert_eq!(
        responses["input"][0]["summary"].as_array().unwrap().len(),
        2
    );
    assert!(responses["input"][0].get("encrypted_content").is_none());
    let session = responses["prompt_cache_key"].as_str().unwrap();
    assert_eq!(responses["model"], "grok-composer-fast");
    assert_eq!(responses["stream"], true);
    assert_eq!(session_header, session);

    let edit = Bytes::from_static(
        br#"{"model":"route","prompt":"edit","image":["data:image/png;base64,AAAA","https://example/b.png"],"mask":"drop"}"#,
    );
    let edit = prepare(Req {
        key: openai(Operation::EditImage),
        stream: false,
        path: "/v1/images/edits",
        body: &edit,
        model: "grok-imagine-image-quality",
        settings: &json!({}),
    });
    let edit: Value = serde_json::from_slice(edit.body()).unwrap();
    assert_eq!(edit["images"][0]["url"], "data:image/png;base64,AAAA");
    assert!(edit.get("mask").is_none());

    let raw = Bytes::from_static(br#"{"request_id":"req_1","video":{"url":"https://cdn/v.mp4"}}"#);
    let shaped = GrokBuildChannel
        .shape_response(ResponseShapeCtx {
            key: openai(Operation::RetrieveVideo),
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &raw,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(shaped["id"], "req_1");
    assert_eq!(shaped["status"], "completed");
    assert_eq!(shaped["url"], "https://cdn/v.mp4");
}
