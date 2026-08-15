use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::{GrokBuildChannel, shape};
use crate::channel::{Channel, PrepareCtx, ShapeCtx};
use crate::protocol::{
    ContentGenerationKind as Kind, Operation, OperationKey, OperationKind, Provider,
};
use crate::routing::RoutingDecision;

fn route(operation: Operation, kind: OperationKind) -> RoutingDecision {
    GrokBuildChannel
        .routing_table()
        .into_iter()
        .find(|(source, _)| source.operation() == operation && source.kind() == kind)
        .map(|(_, decision)| decision)
        .expect("missing Grok Build route")
}

#[test]
fn responses_preserve_native_stream_mode() {
    assert_eq!(
        route(
            Operation::GenerateContent,
            OperationKind::ContentGeneration(Kind::OpenAiResponses),
        ),
        RoutingDecision::Passthrough,
    );
    assert_eq!(
        route(
            Operation::StreamGenerateContent,
            OperationKind::ContentGeneration(Kind::OpenAiResponses),
        ),
        RoutingDecision::Passthrough,
    );
}

#[test]
fn responses_websocket_routes_to_http_responses_stream() {
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        let RoutingDecision::TransformTo(target) = route(
            operation,
            OperationKind::ContentGeneration(Kind::OpenAiResponsesWebSocket),
        ) else {
            panic!("Responses WebSocket should use the existing HTTP Responses transform");
        };
        assert_eq!(target.operation(), Operation::StreamGenerateContent);
        assert_eq!(
            target.kind(),
            OperationKind::ContentGeneration(Kind::OpenAiResponses),
        );
    }
}

#[test]
fn compact_routes_to_non_stream_http_responses() {
    let RoutingDecision::TransformTo(target) = route(
        Operation::CompactContent,
        OperationKind::Provider(Provider::OpenAi),
    ) else {
        panic!("Compact should use the existing OpenAI Compact to Responses transform");
    };
    assert_eq!(
        target,
        OperationKey::content_generation(Operation::GenerateContent, Kind::OpenAiResponses),
    );
}

#[test]
fn routes_public_xai_media_surfaces() {
    for operation in [
        Operation::CreateSpeech,
        Operation::CreateTranscription,
        Operation::CreateImage,
        Operation::EditImage,
        Operation::CreateVideo,
        Operation::RetrieveVideo,
        Operation::EditVideo,
        Operation::ExtendVideo,
    ] {
        assert_eq!(
            route(operation, OperationKind::Provider(Provider::OpenAi)),
            RoutingDecision::Passthrough,
        );
    }
}

#[test]
fn prepares_media_on_public_xai_api_instead_of_chat_proxy() {
    let secret = json!({"access_token": "oauth-token", "sub": "user-1"});
    let settings = json!({});
    let headers = HeaderMap::new();
    for (operation, method, path, expected) in [
        (
            Operation::CreateSpeech,
            Method::POST,
            "/v1/audio/speech",
            "https://api.x.ai/v1/tts",
        ),
        (
            Operation::CreateTranscription,
            Method::POST,
            "/v1/audio/transcriptions",
            "https://api.x.ai/v1/stt",
        ),
        (
            Operation::CreateImage,
            Method::POST,
            "/v1/images/generations",
            "https://api.x.ai/v1/images/generations",
        ),
        (
            Operation::EditImage,
            Method::POST,
            "/v1/images/edits",
            "https://api.x.ai/v1/images/edits",
        ),
        (
            Operation::CreateVideo,
            Method::POST,
            "/v1/videos",
            "https://api.x.ai/v1/videos/generations",
        ),
        (
            Operation::RetrieveVideo,
            Method::GET,
            "/v1/videos/request_1",
            "https://api.x.ai/v1/videos/request_1",
        ),
        (
            Operation::EditVideo,
            Method::POST,
            "/v1/videos/edits",
            "https://api.x.ai/v1/videos/edits",
        ),
        (
            Operation::ExtendVideo,
            Method::POST,
            "/v1/videos/extensions",
            "https://api.x.ai/v1/videos/extensions",
        ),
    ] {
        let request = GrokBuildChannel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                op: OperationKey::provider(operation, Provider::OpenAi),
                stream: false,
                upstream_model_id: "",
                method,
                path,
                query: None,
                headers: &headers,
                body: Bytes::new(),
            })
            .unwrap()
            .into_http()
            .unwrap();
        assert_eq!(request.uri().to_string(), expected);
    }
}

#[test]
fn shapes_openai_media_requests_for_xai() {
    let settings = json!({});
    let mut headers = HeaderMap::new();
    let shape = |operation, body: &'static [u8], headers: &mut HeaderMap| {
        GrokBuildChannel.shape_request(
            Bytes::from_static(body),
            headers,
            &ShapeCtx {
                op: OperationKey::provider(operation, Provider::OpenAi),
                stream: false,
                status: http::StatusCode::OK,
                settings: &settings,
            },
        )
    };

    let speech: Value = serde_json::from_slice(&shape(
        Operation::CreateSpeech,
        br#"{"model":"grok-tts","input":"hello","voice":"ara","response_format":"wav","speed":1.1}"#,
        &mut headers,
    ))
    .unwrap();
    assert_eq!(speech["text"], "hello");
    assert_eq!(speech["voice_id"], "ara");
    assert_eq!(speech["output_format"]["codec"], "wav");
    assert_eq!(speech["speed"], 1.1);
    assert!(speech.get("model").is_none());

    let transcription: Value = serde_json::from_slice(&shape(
        Operation::CreateTranscription,
        br#"{"file":"data:audio/wav;base64,UklGRg==","model":"grok-transcribe","language":"en","response_format":"json"}"#,
        &mut headers,
    ))
    .unwrap();
    assert_eq!(transcription["language"], "en");
    assert!(transcription.get("file").is_some());
    assert!(transcription.get("model").is_none());
    assert!(transcription.get("response_format").is_none());

    let image: Value = serde_json::from_slice(&shape(
        Operation::EditImage,
        br#"{"model":"grok-imagine-image","prompt":"night","image":["data:image/png;base64,AAAA","https://example.com/b.png"],"mask":"data:image/png;base64,AQID"}"#,
        &mut headers,
    ))
    .unwrap();
    assert_eq!(image["images"][0]["url"], "data:image/png;base64,AAAA");
    assert_eq!(image["images"][1]["url"], "https://example.com/b.png");
    assert!(image.get("image").is_none());
    assert!(image.get("mask").is_none());

    let video: Value = serde_json::from_slice(&shape(
        Operation::CreateVideo,
        br#"{"model":"grok-imagine-video","prompt":"cat","seconds":"8","input_reference":"data:image/png;base64,AAAA"}"#,
        &mut headers,
    ))
    .unwrap();
    assert_eq!(video["duration"], 8);
    assert_eq!(video["image"]["url"], "data:image/png;base64,AAAA");
    assert!(video.get("seconds").is_none());
    assert!(video.get("input_reference").is_none());
}

#[test]
fn shapes_xai_video_jobs_as_openai_video_objects() {
    let settings = json!({});
    let ctx = ShapeCtx {
        op: OperationKey::provider(Operation::CreateVideo, Provider::OpenAi),
        stream: false,
        status: http::StatusCode::OK,
        settings: &settings,
    };
    let shaped = GrokBuildChannel.shape_response(
        Bytes::from_static(
            br#"{"request_id":"request_1","status":"done","video":{"url":"https://cdn.example/video.mp4"}}"#,
        ),
        &ctx,
    );
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["id"], "request_1");
    assert_eq!(value["status"], "completed");
    assert_eq!(value["url"], "https://cdn.example/video.mp4");
}

#[test]
fn response_shape_preserves_stream_while_retaining_grok_hygiene() {
    for (stream, expected) in [
        (Some(false), Some(false)),
        (Some(true), Some(true)),
        (None, None),
    ] {
        let mut body = json!({
            "model": "grok-4.5",
            "input": "hello",
            "metadata": {"private": true},
            "previous_response_id": "resp-old",
        });
        if let Some(stream) = stream {
            body["stream"] = Value::Bool(stream);
        }

        let shaped: Value = serde_json::from_slice(&shape::shape_responses_request(Bytes::from(
            body.to_string(),
        )))
        .unwrap();

        assert_eq!(shaped.get("stream").and_then(Value::as_bool), expected);
        assert!(shaped.get("metadata").is_none());
        assert!(shaped.get("previous_response_id").is_none());
    }
}

#[test]
fn prepare_no_longer_opens_an_upstream_responses_websocket() {
    let secret = json!({"access_token": "oauth-token", "sub": "user-1"});
    let settings = json!({});
    let headers = HeaderMap::new();
    let request = GrokBuildChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::content_generation(
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket,
            ),
            stream: true,
            upstream_model_id: "grok-4.5",
            method: Method::GET,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(
                br#"{"type":"response.create","model":"grok-4.5","input":"hello"}"#,
            ),
        })
        .unwrap()
        .into_http()
        .unwrap();

    assert_eq!(request.uri().scheme_str(), Some("https"));
    assert_eq!(request.uri().path(), "/v1/responses");
}
