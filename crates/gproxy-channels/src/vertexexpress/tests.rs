use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, StreamFraming, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::VertexExpressChannel;

const STREAM: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::GeminiGenerateContent,
);
const GENERATE: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::GeminiGenerateContent,
);

#[test]
fn declares_only_express_operations_with_available_pairs() {
    let supports = VertexExpressChannel.descriptor().supports;
    assert_eq!(supports.len(), 9);
    assert_eq!(
        supports
            .iter()
            .filter(|support| support.source == support.target)
            .count(),
        3
    );
    assert!(supports.iter().all(|support| {
        support.target == OperationKey::family(Operation::CountTokens, WireFamily::Gemini)
            || support.target.kind()
                == gproxy_protocol::OperationKind::ContentGeneration(
                    ContentGenerationKind::GeminiGenerateContent,
                )
    }));
    for operation in [
        Operation::ListModels,
        Operation::GetModel,
        Operation::CreateEmbedding,
    ] {
        assert!(!supports.iter().any(|support| {
            support.source == OperationKey::family(operation, WireFamily::Gemini)
        }));
    }
}

#[test]
fn builds_publisher_stream_framings_and_exact_override() {
    let secret = json!({"api_key":"express key"});
    let settings = json!({});
    let body = Bytes::from_static(br#"{"contents":[]}"#);
    let prepare_stream = |query| {
        VertexExpressChannel
            .prepare(PrepareCtx {
                key: STREAM,
                stream: true,
                method: &Method::GET,
                path: "/v1beta/models/route:streamGenerateContent",
                query,
                headers: &HeaderMap::new(),
                body: &body,
                upstream_model: "publishers/google/models/gemini-3-flash",
                provider_settings: &settings,
                secret: &secret,
            })
            .unwrap()
    };
    let array = prepare_stream(None);
    assert_eq!(
        array.request.uri(),
        "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-3-flash:streamGenerateContent?key=express%20key"
    );
    assert_eq!(array.framing, Some(StreamFraming::JsonArray));
    let sse = prepare_stream(Some("alt=sse&key=downstream"));
    assert!(
        sse.request
            .uri()
            .to_string()
            .contains("?alt=sse&key=express%20key")
    );
    assert_eq!(sse.framing, Some(StreamFraming::Sse));

    let override_settings = json!({
        "base_url":"https://unused.example",
        "endpoints":{"gemini_count_tokens":"https://count.example/{model}?fixed=1"}
    });
    let count = VertexExpressChannel
        .prepare(PrepareCtx {
            key: OperationKey::family(Operation::CountTokens, WireFamily::Gemini),
            stream: false,
            method: &Method::POST,
            path: "/v1beta/models/route:countTokens",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "gemini/model one",
            provider_settings: &override_settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        count.request.uri(),
        "https://count.example/gemini%2Fmodel%20one?fixed=1&key=express%20key"
    );
}

#[test]
fn strips_only_store_and_normalizes_vertex_response_quirks() {
    let secret = json!({"api_key":"key"});
    let settings = json!({});
    let body = Bytes::from_static(
        br#"{"model":"route","contents":[],"store":true,"generationConfig":{"temperature":0.4},"future":7}"#,
    );
    let prepared = VertexExpressChannel
        .prepare(PrepareCtx {
            key: GENERATE,
            stream: false,
            method: &Method::POST,
            path: "/v1beta/models/route:generateContent",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "gemini-3-flash",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert!(shaped.get("store").is_none());
    assert_eq!(shaped["model"], "models/gemini-3-flash");
    assert_eq!(shaped["generationConfig"]["temperature"], 0.4);
    assert_eq!(shaped["future"], 7);

    let raw = Bytes::from(
        json!({
            "candidates":[{"citationMetadata":{"citations":[{"uri":"x"}]}}],
            "promptFeedback":{"blockReason":"BLOCKED_REASON_UNSPECIFIED"}
        })
        .to_string(),
    );
    let normalized = VertexExpressChannel
        .shape_response(ResponseShapeCtx {
            key: GENERATE,
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &raw,
        })
        .unwrap();
    let normalized: Value = serde_json::from_slice(&normalized).unwrap();
    assert_eq!(
        normalized["candidates"][0]["citationMetadata"]["citationSources"][0]["uri"],
        "x"
    );
    assert_eq!(
        normalized["promptFeedback"]["blockReason"],
        "BLOCK_REASON_UNSPECIFIED"
    );
}
