//! images family: OpenAI Images requests converted to Gemini `generateContent`
//! or the Responses image tool, driven through the per-operation entry points.

mod support;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use support::*;

/// A 1x1 PNG.
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

fn png_bytes() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap()
}

fn gemini_image_reply() -> Reply {
    json_reply(
        StatusCode::OK,
        json!({
            "candidates": [{"index": 0, "finishReason": "STOP", "content": {"role": "model",
                "parts": [{"inlineData": {"mimeType": "image/png", "data": PNG}}]}}],
            "responseId": "gemini-r", "modelVersion": "gemini-actual",
            "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 11, "totalTokenCount": 18}
        }),
    )
}

fn json_request(path: &str, body: Value) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: None,
        headers,
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

/// A multipart edit form with one PNG `image` part and text fields.
fn edit_form(fields: &[(&str, &str)]) -> WireRequest<HttpBody> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!("--XYZ\r\ncontent-disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(
        b"--XYZ\r\ncontent-disposition: form-data; name=\"image\"; filename=\"in.png\"\r\ncontent-type: image/png\r\n\r\n",
    );
    body.extend_from_slice(&png_bytes());
    body.extend_from_slice(b"\r\n--XYZ--\r\n");
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=XYZ"),
    );
    WireRequest {
        method: Method::POST,
        path: "/v1/images/edits".into(),
        query: None,
        headers,
        body: HttpBody::Bytes(Bytes::from(body)),
    }
}

async fn send(
    h: &Harness,
    provider: &str,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> gproxy_core::CoreResult<gproxy_core::HttpExecution> {
    let ctx = h.context_for(
        provider,
        OperationKey {
            operation,
            dialect: Dialect::OpenAi,
        },
        "r",
        1,
        None,
    );
    match operation {
        Operation::CreateImage => h.core.create_image(ctx, request).await,
        Operation::EditImage => h.core.edit_image(ctx, request).await,
        other => panic!("not an images operation: {other:?}"),
    }
}

async fn run(
    h: &Harness,
    provider: &str,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> (StatusCode, Value) {
    let execution = send(h, provider, operation, request).await.unwrap();
    let (response, completion) = execution.into_parts();
    let body = read(response.body).await;
    let _ = completion.await;
    (
        response.status,
        serde_json::from_str(&body).unwrap_or(Value::String(body)),
    )
}

#[tokio::test]
async fn create_makes_one_gemini_call_per_image_and_returns_b64_json() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    h.script(vec![gemini_image_reply(), gemini_image_reply()]);
    let (status, body) = run(
        &h,
        "gemini",
        Operation::CreateImage,
        json_request(
            "/v1/images/generations",
            json!({"prompt": "draw a red square", "n": 2, "model": "gpt-image-1"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["b64_json"], PNG);
    assert_eq!(data[1]["b64_json"], PNG);
    assert!(body["url"].is_null());
    assert_eq!(body["output_format"], "png");
    assert!(body["created"].as_i64().unwrap() > 0);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    for line in &seen {
        assert!(
            line.starts_with(
                "POST https://gemini.example/v1beta/models/gpt-x:generateContent auth=Bearer k-gemini"
            ),
            "{line}"
        );
        assert!(line.contains("draw a red square"), "{line}");
    }
}

#[tokio::test]
async fn multipart_edit_sends_the_input_image_inline() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    h.script(vec![gemini_image_reply()]);
    let (status, body) = run(
        &h,
        "gemini",
        Operation::EditImage,
        edit_form(&[("prompt", "make it blue"), ("n", "1")]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"][0]["b64_json"], PNG);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].contains("make it blue"), "{}", seen[0]);
    assert!(
        seen[0].contains(&format!("\"data\":\"{PNG}\"")) && seen[0].contains("image/png"),
        "input image must reach Gemini as inline data: {}",
        seen[0]
    );
}

#[tokio::test]
async fn url_delivery_and_claude_targets_are_refused_before_any_call() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    // No `PublicationUrl` on this core: URL delivery is refused up front.
    let Err(error) = send(
        &h,
        "gemini",
        Operation::CreateImage,
        json_request(
            "/v1/images/generations",
            json!({"prompt": "draw", "response_format": "url"}),
        ),
    )
    .await
    else {
        panic!("url delivery must be refused")
    };
    assert!(error.to_string().contains("b64_json"), "{error}");
    assert!(error.to_string().contains("PublicationUrl"), "{error}");

    let Err(error) = send(
        &h,
        "claude",
        Operation::CreateImage,
        json_request("/v1/images/generations", json!({"prompt": "draw"})),
    )
    .await
    else {
        panic!("Claude has no image generation")
    };
    assert!(error.to_string().contains("image generation"), "{error}");
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn url_delivery_publishes_through_the_host_link_builder() {
    let dir = tempfile::tempdir().unwrap();
    let operator = gproxy_file::filesystem(dir.path().to_str().unwrap()).unwrap();
    let h = harness_with_publication(
        full(),
        "round_robin",
        Some(operator),
        Some(std::sync::Arc::new(FixedLinks(Some(
            "https://files.example",
        )))),
    )
    .await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    h.script(vec![gemini_image_reply(), gemini_image_reply()]);
    let (status, body) = run(
        &h,
        "gemini",
        Operation::CreateImage,
        json_request(
            "/v1/images/generations",
            json!({"prompt": "draw", "n": 2, "response_format": "url"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    let mut ids = Vec::new();
    for image in data {
        assert!(image["b64_json"].is_null());
        let url = image["url"].as_str().unwrap();
        let id = url
            .strip_prefix("https://files.example/")
            .unwrap_or_else(|| panic!("host link expected: {url}"));
        ids.push(id.to_owned());
    }
    assert_ne!(ids[0], ids[1]);
    // The host's download route serves what the link names.
    for id in &ids {
        let publication = h.core.read_publication(id).await.unwrap().unwrap();
        assert_eq!(publication.metadata.mime.as_deref(), Some("image/png"));
        assert_eq!(publication.metadata.length, Some(png_bytes().len() as u64));
        let bytes = match publication.body {
            HttpBody::Bytes(bytes) => bytes,
            _ => panic!("stored body is buffered"),
        };
        assert_eq!(bytes.as_ref(), png_bytes().as_slice());
    }
    assert_eq!(h.client.seen.lines().len(), 2);
}

#[tokio::test]
async fn upstream_rejection_keeps_its_status_and_body() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    h.script(vec![json_reply(
        StatusCode::TOO_MANY_REQUESTS,
        json!({"error": {"code": 429, "message": "quota"}}),
    )]);
    let (status, body) = run(
        &h,
        "gemini",
        Operation::CreateImage,
        json_request("/v1/images/generations", json!({"prompt": "draw"})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["message"], "quota");
}
