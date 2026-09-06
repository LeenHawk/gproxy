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
    assert_eq!(supports.len(), 17);
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
    for operation in [Operation::CreateEmbedding, Operation::BatchCreateEmbedding] {
        assert!(supports.iter().any(|support| {
            support.source == OperationKey::family(operation, WireFamily::Gemini)
        }));
    }
    assert!(!supports.iter().any(|support| {
        support.source == OperationKey::family(Operation::CreateVideo, WireFamily::OpenAi)
    }));
    assert_eq!(VertexChannel.refresh_due(&json!({})), Some(i64::MIN));
}

#[test]
fn embedding_batch_uses_prediction_wire_and_preserves_order_and_usage() {
    use gproxy_channel_api::UsageCtx;
    let key = OperationKey::family(Operation::BatchCreateEmbedding, WireFamily::Gemini);
    let body = Bytes::from_static(br#"{"requests":[{"content":{"parts":[{"text":"first"}]},"outputDimensionality":2},{"content":{"parts":[{"text":"second"}]},"outputDimensionality":2}]}"#);
    let prepared = VertexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key,
            stream: false,
            method: &Method::POST,
            path: "/v1beta/models/text-embedding-005:batchEmbedContents",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "text-embedding-005",
            provider_settings: &json!({}),
            secret: &secret(),
        })
        .unwrap();
    assert_eq!(
        prepared.request.uri().path(),
        "/v1/projects/project-1/locations/us-central1/publishers/google/models/text-embedding-005:predict"
    );
    let request: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(
        request["instances"],
        json!([{"content":"first"},{"content":"second"}])
    );
    assert_eq!(request["parameters"]["outputDimensionality"], 2);
    let response = Bytes::from_static(br#"{"predictions":[{"embeddings":{"values":[0.25,0.5],"statistics":{"token_count":3}}},{"embeddings":{"values":[0.5,0.25],"statistics":{"token_count":4}}}]}"#);
    let headers = HeaderMap::new();
    let usage = VertexChannel
        .extract_usage(UsageCtx {
            key,
            request_body: &body,
            response_headers: &headers,
            response_body: &response,
        })
        .unwrap();
    assert_eq!(usage.input_tokens, 7);
    let normalized = VertexChannel
        .shape_response(ResponseShapeCtx {
            key,
            status: StatusCode::OK,
            headers: &headers,
            body: &response,
        })
        .unwrap();
    let normalized: Value = serde_json::from_slice(&normalized).unwrap();
    assert_eq!(normalized["embeddings"][1]["values"], json!([0.5, 0.25]));
    assert_eq!(normalized["usageMetadata"]["promptTokenCount"], 7);
}

#[test]
fn single_embeddings_choose_legacy_prediction_or_native_embed_content() {
    let key = OperationKey::family(Operation::CreateEmbedding, WireFamily::Gemini);
    for (model, verb, reply) in [
        (
            "gemini-embedding-001",
            ":predict",
            br#"{"predictions":[{"embeddings":{"values":[0.5],"statistics":{"token_count":3}}}]}"#
                .as_slice(),
        ),
        (
            "gemini-embedding-2",
            ":embedContent",
            br#"{"embedding":{"values":[0.5]},"usageMetadata":{"promptTokenCount":3}}"#.as_slice(),
        ),
    ] {
        let body = Bytes::from_static(
            br#"{"model":"models/alias","content":{"parts":[{"text":"input"}]}}"#,
        );
        let prepared = VertexChannel
            .prepare(PrepareCtx {
                session_id: None,
                key,
                stream: false,
                method: &Method::POST,
                path: "/v1beta/models/alias:embedContent",
                query: None,
                headers: &HeaderMap::new(),
                body: &body,
                upstream_model: model,
                provider_settings: &json!({}),
                secret: &secret(),
            })
            .unwrap();
        assert!(
            prepared
                .request
                .uri()
                .path()
                .ends_with(&format!("{model}{verb}"))
        );
        let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
        assert!(shaped.get("model").is_none());
        let reply = Bytes::copy_from_slice(reply);
        let normalized = VertexChannel
            .shape_response(ResponseShapeCtx {
                key,
                status: StatusCode::OK,
                headers: &HeaderMap::new(),
                body: &reply,
            })
            .unwrap();
        let normalized: Value = serde_json::from_slice(&normalized).unwrap();
        assert_eq!(normalized["embedding"]["values"], json!([0.5]));
        assert_eq!(normalized["usageMetadata"]["promptTokenCount"], 3);
    }
}

#[test]
fn builds_regional_stream_and_exact_model_override_urls() {
    let secret = secret();
    let settings = json!({});
    let body = Bytes::from_static(br#"{"contents":[]}"#);
    let prepared = VertexChannel
        .prepare(PrepareCtx {
            session_id: None,
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
            session_id: None,
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
            session_id: None,
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

    let video = Bytes::from_static(
        br#"{"model":"route","prompt":"fly","input_reference":"data:image/png;base64,abc","seconds":"8","n":2,"generate_audio":true}"#,
    );
    let video = VertexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::CreateVideo, WireFamily::OpenAi),
            stream: false,
            method: &Method::POST,
            path: "/v1/videos",
            query: None,
            headers: &HeaderMap::new(),
            body: &video,
            upstream_model: "veo-3",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let video: Value = serde_json::from_slice(video.request.body()).unwrap();
    assert_eq!(video["instances"][0]["prompt"], "fly");
    assert_eq!(video["instances"][0]["image"]["mimeType"], "image/png");
    assert_eq!(video["parameters"]["durationSeconds"], 8);
    assert_eq!(video["parameters"]["sampleCount"], 2);
    assert_eq!(video["parameters"]["generateAudio"], true);

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
            session_id: None,
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
