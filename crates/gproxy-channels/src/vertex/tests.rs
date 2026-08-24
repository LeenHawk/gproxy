use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::VertexChannel;

const GEMINI_STREAM: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::GeminiGenerateContent,
);
const CLAUDE_MESSAGES: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::ClaudeMessages,
);

fn secret() -> Value {
    json!({
        "project_id":"project-1",
        "location":"us-central1",
        "access_token":"token"
    })
}

#[test]
fn declares_only_operations_with_verified_paths_and_pairs() {
    let supports = VertexChannel.descriptor().supports;
    assert_eq!(supports.len(), 15);
    assert!(supports.iter().any(|support| {
        support.source == OperationKey::family(Operation::CountTokens, WireFamily::Claude)
            && support.source == support.target
    }));
    assert!(supports.iter().any(|support| {
        support.source
            == OperationKey::content(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            )
            && support.target
                == OperationKey::content(
                    Operation::GenerateContent,
                    ContentGenerationKind::GeminiGenerateContent,
                )
    }));
    for unsupported in [Operation::CreateEmbedding, Operation::BatchCreateEmbedding] {
        assert!(!supports.iter().any(|support| {
            support.source == OperationKey::family(unsupported, WireFamily::Gemini)
        }));
    }
    assert!(!supports.iter().any(|support| {
        support.source == OperationKey::family(Operation::CreateVideo, WireFamily::OpenAi)
    }));
    assert_eq!(VertexChannel.refresh_due(&json!({})), Some(i64::MIN));
}

#[test]
fn builds_regional_stream_and_exact_model_override_urls() {
    let secret = secret();
    let settings = json!({});
    let body = Bytes::from_static(br#"{"contents":[]}"#);
    let prepared = VertexChannel
        .prepare(PrepareCtx {
            key: GEMINI_STREAM,
            stream: true,
            method: &Method::GET,
            path: "/v1beta/models/route:streamGenerateContent",
            query: Some("alt=json&key=downstream"),
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "publishers/google/models/gemini-3-flash",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        prepared.request.uri(),
        "https://us-central1-aiplatform.googleapis.com/v1beta1/projects/project-1/locations/us-central1/publishers/google/models/gemini-3-flash:streamGenerateContent?alt=sse"
    );
    assert_eq!(prepared.request.method(), Method::POST);
    assert_eq!(prepared.framing, Some(gproxy_protocol::StreamFraming::Sse));
    assert_eq!(prepared.request.headers()["authorization"], "Bearer token");

    let override_settings = json!({
        "base_url":"https://unused.example",
        "endpoints":{
            "gemini_get_model":"https://models.example/{model}?fixed=1"
        }
    });
    let empty = Bytes::new();
    let model = VertexChannel
        .prepare(PrepareCtx {
            key: OperationKey::family(Operation::GetModel, WireFamily::Gemini),
            stream: false,
            method: &Method::GET,
            path: "/v1beta/models/route",
            query: None,
            headers: &HeaderMap::new(),
            body: &empty,
            upstream_model: "gemini/model one",
            provider_settings: &override_settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        model.request.uri(),
        "https://models.example/gemini%2Fmodel%20one?fixed=1"
    );
}

#[test]
fn shapes_partner_claude_and_round_trips_video_operation_aliases() {
    let secret = secret();
    let settings = json!({});
    let mut headers = HeaderMap::new();
    headers.insert("anthropic-beta", "feature-x".parse().unwrap());
    let body =
        Bytes::from_static(br#"{"model":"route","max_tokens":16,"messages":[],"future":true}"#);
    let claude = VertexChannel
        .prepare(PrepareCtx {
            key: CLAUDE_MESSAGES,
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "claude-sonnet-4-6@20260301",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert!(
        claude
            .request
            .uri()
            .path()
            .ends_with("/claude-sonnet-4-6%4020260301:rawPredict")
    );
    assert_eq!(claude.request.headers()["anthropic-beta"], "feature-x");
    let shaped: Value = serde_json::from_slice(claude.request.body()).unwrap();
    assert!(shaped.get("model").is_none());
    assert_eq!(shaped["anthropic_version"], "vertex-2023-10-16");
    assert_eq!(shaped["future"], true);

    let operation =
        "projects/project-1/locations/us-central1/publishers/google/models/veo-3/operations/op-1";
    let raw = Bytes::from(json!({"name":operation,"done":false,"future":7}).to_string());
    let outward = VertexChannel
        .shape_response(ResponseShapeCtx {
            key: OperationKey::family(Operation::CreateVideo, WireFamily::Gemini),
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &raw,
        })
        .unwrap();
    let outward: Value = serde_json::from_slice(&outward).unwrap();
    let alias = outward["name"]
        .as_str()
        .unwrap()
        .strip_prefix("operations/")
        .unwrap();
    assert_eq!(outward["vertexOperationName"], operation);
    let path = format!("/v1beta/operations/{alias}");
    let empty = Bytes::new();
    let poll = VertexChannel
        .prepare(PrepareCtx {
            key: OperationKey::family(Operation::RetrieveVideo, WireFamily::Gemini),
            stream: false,
            method: &Method::GET,
            path: &path,
            query: None,
            headers: &HeaderMap::new(),
            body: &empty,
            upstream_model: "",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert!(
        poll.request
            .uri()
            .path()
            .ends_with("/veo-3:fetchPredictOperation")
    );
    assert_eq!(poll.request.method(), Method::POST);
    let poll_body: Value = serde_json::from_slice(poll.request.body()).unwrap();
    assert_eq!(poll_body["operationName"], operation);
}
