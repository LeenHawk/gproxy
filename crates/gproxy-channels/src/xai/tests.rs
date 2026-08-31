use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, ResponseShapeCtx, UsageCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::XaiChannel;

fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

struct Req<'a> {
    key: OperationKey,
    stream: bool,
    path: &'a str,
    headers: &'a HeaderMap,
    body: &'a Bytes,
    model: &'a str,
    settings: &'a Value,
}

fn prepare(req: Req<'_>) -> Result<http::Request<Bytes>, gproxy_channel_api::ChannelError> {
    XaiChannel
        .prepare(PrepareCtx {
            key: req.key,
            stream: req.stream,
            method: &Method::POST,
            path: req.path,
            query: Some("key=downstream&ignored=1"),
            headers: req.headers,
            body: req.body,
            upstream_model: req.model,
            provider_settings: req.settings,
            secret: &json!({"api_key":"xai-test"}),
        })
        .map(|prepared| prepared.request)
}

#[test]
fn descriptor_declares_native_and_current_transform_routes() {
    let descriptor = XaiChannel.descriptor();
    assert_eq!(descriptor.id, "xai");
    assert_eq!(descriptor.supports.len(), 21);
    assert_eq!(
        descriptor
            .supports
            .iter()
            .filter(|support| support.source == support.target)
            .count(),
        15
    );
    for (source, target) in [
        (
            family(Operation::ListModels, WireFamily::Claude),
            family(Operation::ListModels, WireFamily::OpenAi),
        ),
        (
            OperationKey::content(Operation::GenerateContent, Kind::ClaudeMessages),
            OperationKey::content(Operation::GenerateContent, Kind::OpenAiResponses),
        ),
        (
            OperationKey::content(
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
            OperationKey::content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        ),
    ] {
        assert!(
            descriptor
                .supports
                .iter()
                .any(|support| support.source == source && support.target == target)
        );
    }
}

#[test]
fn prepare_resolves_default_and_exact_urls_with_owned_auth_headers() {
    let mut headers = HeaderMap::new();
    headers.insert("x-grok-conv-id", "conversation-1".parse().unwrap());
    headers.insert("cookie", "drop=me".parse().unwrap());
    let body = Bytes::from_static(br#"{"model":"route","input":"hi"}"#);
    let response = prepare(Req {
        key: OperationKey::content(Operation::GenerateContent, Kind::OpenAiResponses),
        stream: false,
        path: "/v1/responses",
        headers: &headers,
        body: &body,
        model: "grok-4.5",
        settings: &json!({}),
    })
    .unwrap();
    assert_eq!(response.uri(), "https://api.x.ai/v1/responses");
    assert_eq!(response.headers()["authorization"], "Bearer xai-test");
    assert_eq!(response.headers()["x-grok-conv-id"], "conversation-1");
    assert!(response.headers().get("cookie").is_none());

    let video = Bytes::from_static(br#"{"prompt":"cat","model":"route"}"#);
    let exact = prepare(Req {
        key: family(Operation::CreateVideo, WireFamily::OpenAi),
        stream: false,
        path: "/v1/videos",
        headers: &HeaderMap::new(),
        body: &video,
        model: "grok-imagine-video",
        settings: &json!({
            "base_url":"https://unused.example",
            "endpoints":{"openai_video_create":"https://relay.example/video"}
        }),
    })
    .unwrap();
    assert_eq!(exact.uri(), "https://relay.example/video");
}

#[test]
fn prepare_shapes_image_audio_and_video_without_model_enrichment() {
    let image_key = family(Operation::EditImage, WireFamily::OpenAi);
    let image_json = Bytes::from_static(br#"{"model":"route","prompt":"edit"}"#);
    assert!(
        prepare(Req {
            key: image_key,
            stream: true,
            path: "/v1/images/edits",
            headers: &HeaderMap::new(),
            body: &image_json,
            model: "grok-imagine-image-quality",
            settings: &json!({}),
        })
        .is_err()
    );

    let mut multipart_headers = HeaderMap::new();
    multipart_headers.insert(
        "content-type",
        "multipart/form-data; boundary=x".parse().unwrap(),
    );
    let multipart = Bytes::from_static(
        b"--x\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nedit\r\n--x\r\nContent-Disposition: form-data; name=\"image\"; filename=\"a.png\"\r\nContent-Type: image/png\r\n\r\n\x00\xff\r\n--x--\r\n",
    );
    let image = prepare(Req {
        key: image_key,
        stream: false,
        path: "/v1/images/edits",
        headers: &multipart_headers,
        body: &multipart,
        model: "grok-imagine-image-quality",
        settings: &json!({}),
    })
    .unwrap();
    let image: Value = serde_json::from_slice(image.body()).unwrap();
    assert!(
        image["image"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );

    let speech = Bytes::from_static(
        br#"{"model":"grok-tts","input":"hello","voice":"ara","response_format":"mp3"}"#,
    );
    let speech = prepare(Req {
        key: family(Operation::CreateSpeech, WireFamily::OpenAi),
        stream: false,
        path: "/v1/audio/speech",
        headers: &HeaderMap::new(),
        body: &speech,
        model: "ignored",
        settings: &json!({}),
    })
    .unwrap();
    let speech: Value = serde_json::from_slice(speech.body()).unwrap();
    assert_eq!(speech["text"], "hello");
    assert_eq!(speech["output_format"]["codec"], "mp3");
    assert!(speech.get("model").is_none());

    let pcm_body =
        Bytes::from_static(br#"{"input":"hello","voice":"ara","response_format":"pcm"}"#);
    let pcm = prepare(Req {
        key: family(Operation::CreateSpeech, WireFamily::OpenAi),
        stream: false,
        path: "/v1/audio/speech",
        headers: &HeaderMap::new(),
        body: &pcm_body,
        model: "ignored",
        settings: &json!({}),
    })
    .unwrap();
    let response_headers = HeaderMap::new();
    let response_body = vec![0_u8; 48_000];
    let usage = XaiChannel
        .extract_usage(UsageCtx {
            key: family(Operation::CreateSpeech, WireFamily::OpenAi),
            request_body: pcm.body(),
            response_headers: &response_headers,
            response_body: &response_body,
        })
        .unwrap();
    assert_eq!(usage.metrics["audio_seconds"], rust_decimal::Decimal::ONE);

    let raw = Bytes::from_static(
        br#"{"request_id":"req_1","status":"done","video":{"url":"https://cdn/v.mp4"}}"#,
    );
    let shaped = XaiChannel
        .shape_response(ResponseShapeCtx {
            key: family(Operation::RetrieveVideo, WireFamily::OpenAi),
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

#[test]
fn response_restores_documented_grok_model_metadata() {
    let body = Bytes::from_static(
        br#"{"object":"list","data":[{"id":"grok-4.6","object":"model"},{"id":"other","object":"model"}]}"#,
    );
    let shaped = XaiChannel
        .shape_response(ResponseShapeCtx {
            key: family(Operation::ListModels, WireFamily::OpenAi),
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &body,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(shaped["data"][0]["display_name"], "Grok 4.6");
    assert_eq!(shaped["data"][0]["context_length"], 500_000);
    assert_eq!(
        shaped["data"][0]["supported_parameters"],
        json!(["reasoning"])
    );
    assert_eq!(shaped["data"][0]["thinking_supported"], true);
    assert!(shaped["data"][1].get("display_name").is_none());
}
