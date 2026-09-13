use gproxy_protocol::{
    WireRequest,
    connection::{HeaderMap, Method},
    gemini::video::{GenerateVideoResponse, PredictLongRunningRequestBody, VideoOperation},
};
use serde_json::json;

#[test]
fn predict_long_running_serializes_developer_path_and_mldev_body() {
    let body: PredictLongRunningRequestBody = serde_json::from_value(json!({
        "instances":[{"prompt":"a red kite","image":{"bytesBase64Encoded":"AQI=","mimeType":"image/png"},"referenceImages":[{"image":{"bytesBase64Encoded":"AwQ=","mimeType":"image/jpeg"},"referenceType":"ASSET"}]}],
        "parameters":{"sampleCount":2,"durationSeconds":8,"aspectRatio":"16:9","resolution":"720p","personGeneration":"allow_adult","negativePrompt":"blur","enhancePrompt":true},
        "webhookConfig":{"uris":["https://example.test/hook"],"userMetadata":{"job":"42"}}
    })).unwrap();
    assert_eq!(body.instances[0].prompt.as_deref(), Some("a red kite"));
    assert_eq!(body.parameters.as_ref().unwrap().sample_count, Some(2));
    assert_eq!(
        body.webhook_config
            .as_ref()
            .unwrap()
            .user_metadata
            .as_ref()
            .unwrap()["job"],
        "42"
    );
    assert!(body.rest.is_empty());
    let request = WireRequest {
        method: Method::POST,
        path: "/v1beta/models/veo-3.0-generate-preview:predictLongRunning".into(),
        query: None,
        headers: HeaderMap::new(),
        body,
    };
    assert_eq!(
        request.path,
        "/v1beta/models/veo-3.0-generate-preview:predictLongRunning"
    );
    let value = serde_json::to_value(request.body).unwrap();
    assert_eq!(value["parameters"]["sampleCount"], 2);
    assert_eq!(value["parameters"]["durationSeconds"], 8);
    assert_eq!(value["webhookConfig"]["userMetadata"]["job"], "42");
    assert_eq!(
        value["instances"][0]["referenceImages"][0]["referenceType"],
        "ASSET"
    );
    assert!(value.get("model").is_none());
}

#[test]
fn operation_decodes_pending_success_filtered_and_error_states() {
    let pending: VideoOperation = serde_json::from_value(
        json!({"name":"operations/abc","done":false,"metadata":{"progress":42}}),
    )
    .unwrap();
    assert_eq!(pending.name.as_deref(), Some("operations/abc"));
    assert_eq!(pending.done, Some(false));
    assert!(pending.response.is_none());

    let completed: VideoOperation = serde_json::from_value(json!({"name":"operations/abc","done":true,"response":{"generateVideoResponse":{"generatedSamples":[{"video":{"uri":"https://cdn.test/v.mp4","encodedVideo":"AQI=","encoding":"video/mp4"}}],"raiMediaFilteredCount":1,"raiMediaFilteredReasons":["safety"]}}})).unwrap();
    assert!(completed.rest.is_empty());
    let response: GenerateVideoResponse =
        completed.response.unwrap().generate_video_response.unwrap();
    assert_eq!(
        response.generated_samples.unwrap()[0]
            .video
            .as_ref()
            .unwrap()
            .uri
            .as_deref(),
        Some("https://cdn.test/v.mp4")
    );
    assert_eq!(response.rai_media_filtered_count, Some(1));

    let failed: VideoOperation = serde_json::from_value(json!({"name":"operations/abc","done":true,"error":{"code":3,"message":"bad prompt","details":[{"@type":"type.googleapis.com/google.rpc.BadRequest","field":"prompt"}]}})).unwrap();
    assert_eq!(failed.error.unwrap().code, Some(3));
}

#[test]
fn declared_video_fields_cover_sources_aliases_and_arbitrary_metadata() {
    use gproxy_protocol::wire::DeclaredFields;
    let input:PredictLongRunningRequestBody=serde_json::from_value(json!({
        "instances":[{"prompt":"extend","image":{"bytesBase64Encoded":"AQI=","mimeType":"image/png","foreign":1},"video":{"uri":"https://example/video","encodedVideo":"AwQ=","encoding":"video/mp4","foreign":2},"lastFrame":{"bytesBase64Encoded":"BQY=","mimeType":"image/jpeg"},"referenceImages":[{"referenceType":"ASSET","image":{"bytesBase64Encoded":"Bwg=","mimeType":"image/png"}}],"foreign":3}],
        "webhook_config":{"uris":["https://example/hook"],"user_metadata":{"future":{"nested":true}},"foreign":4},"foreign":5
    })).unwrap();
    let input = input.into_declared();
    let instance = &input.instances[0];
    assert_eq!(
        instance.video.as_ref().unwrap().encoded_video.as_deref(),
        Some("AwQ=")
    );
    assert_eq!(
        instance.video.as_ref().unwrap().encoding.as_deref(),
        Some("video/mp4")
    );
    assert_eq!(
        instance
            .last_frame
            .as_ref()
            .unwrap()
            .bytes_base64_encoded
            .as_deref(),
        Some("BQY=")
    );
    assert_eq!(instance.reference_images.as_ref().unwrap().len(), 1);
    assert!(instance.rest.is_empty());
    assert!(instance.image.as_ref().unwrap().rest.is_empty());
    assert!(instance.video.as_ref().unwrap().rest.is_empty());
    let webhook = input.webhook_config.as_ref().unwrap();
    assert!(webhook.rest.is_empty());
    assert_eq!(
        webhook.user_metadata.as_ref().unwrap()["future"]["nested"],
        true
    );
    let wire = serde_json::to_value(input).unwrap();
    assert!(wire.get("webhook_config").is_none());
    assert_eq!(
        wire["webhookConfig"]["userMetadata"]["future"]["nested"],
        true
    );

    let input:VideoOperation=serde_json::from_value(json!({"name":"models/veo/operations/id","done":true,"metadata":{"future":1},"error":{"code":3,"message":"bad","details":[{"@type":"type.googleapis.com/google.rpc.BadRequest","future":2}],"foreign":3},"foreign":4})).unwrap();
    let input = input.into_declared();
    assert_eq!(input.metadata.unwrap()["future"], 1);
    let error = input.error.unwrap();
    assert_eq!(error.message.as_deref(), Some("bad"));
    assert_eq!(error.details.unwrap()[0]["future"], 2);
    assert!(error.rest.is_empty());
    assert!(input.rest.is_empty());
}
