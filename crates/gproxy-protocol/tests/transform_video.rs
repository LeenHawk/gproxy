use std::collections::BTreeMap;

use gproxy_protocol::{
    openai::video as o,
    transform::{TransformErrorKind, video::*},
};
use serde_json::json;

fn resource(reference: &str) -> ResolvedVideoResource {
    ResolvedVideoResource {
        reference: reference.into(),
        url: Some(format!("https://cdn.test/{}", reference)),
        bytes_base64_encoded: Some("AQI=".into()),
        mime_type: Some("image/png".into()),
    }
}

fn source() -> o::CreateVideoRequestBody {
    serde_json::from_value(json!({
        "model":"google/veo-3.1",
        "prompt":"sunset over water",
        "duration":8,
        "aspect_ratio":"16:9",
        "resolution":"1080p",
        "frame_images":[{"type":"image_url","image_url":{"url":"first"},"frame_type":"first_frame"}],
        "input_references":[{"type":"image_url","image_url":{"url":"ref"}}]
    }))
    .unwrap()
}

#[test]
fn openai_request_maps_real_veo_fields_and_retains_facts() {
    let mut resources = BTreeMap::new();
    resources.insert("first".into(), resource("first"));
    resources.insert("ref".into(), resource("ref"));
    let converted = openai_to_gemini_request(source(), "veo-3.1-generate", &resources)
        .unwrap()
        .value;
    assert_eq!(
        converted.context.source_model.as_deref(),
        Some("google/veo-3.1")
    );
    assert_eq!(converted.context.target_model, "veo-3.1-generate");
    assert_eq!(
        converted.body.instances[0].prompt.as_deref(),
        Some("sunset over water")
    );
    assert_eq!(
        converted.body.instances[0]
            .image
            .as_ref()
            .unwrap()
            .mime_type
            .as_deref(),
        Some("image/png")
    );
    let parameters = converted.body.parameters.unwrap();
    assert_eq!(parameters.duration_seconds, Some(8));
    assert_eq!(parameters.aspect_ratio.as_deref(), Some("16:9"));
    assert_eq!(parameters.resolution.as_deref(), Some("1080p"));
    assert!(converted.body.webhook_config.is_none());
}

#[test]
fn gemini_request_requires_published_resource_facts() {
    let body: g::PredictLongRunningRequestBody = serde_json::from_value(json!({
        "instances":[{"prompt":"p","image":{"bytesBase64Encoded":"AQI=","mimeType":"image/png"}}],
        "parameters":{"durationSeconds":4,"aspectRatio":"9:16","resolution":"720p"}
    }))
    .unwrap();
    let error =
        gemini_to_openai_request(body.clone(), "openrouter/veo", &BTreeMap::new()).unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::MissingMetadata);
    let mut resources = BTreeMap::new();
    resources.insert("AQI=".into(), resource("AQI="));
    let converted = gemini_to_openai_request(body, "openrouter/veo", &resources)
        .unwrap()
        .value;
    assert_eq!(converted.body.model, "openrouter/veo");
    assert_eq!(converted.body.duration, Some(4));
    assert_eq!(
        converted.body.aspect_ratio,
        Some(o::VideoAspectRatio::R9x16)
    );
    assert_eq!(converted.body.resolution, Some(o::VideoResolution::P720));
    assert_eq!(
        converted.body.frame_images.unwrap()[0].image_url.url,
        "https://cdn.test/AQI="
    );
}

#[test]
fn operation_response_uses_actual_identity_and_urls() {
    let operation: g::VideoOperation = serde_json::from_value(json!({
        "name":"operations/real-42",
        "done":true,
        "response":{"generateVideoResponse":{"generatedSamples":[{"video":{"uri":"https://files.test/video.mp4"}}]}}
    }))
    .unwrap();
    let converted = gemini_operation_to_openai_response(
        operation,
        &OpenAiVideoResponseContext {
            operation_name: "operations/real-42".into(),
            polling_url: "https://proxy.test/videos/real-42".into(),
            resources: BTreeMap::new(),
        },
    )
    .unwrap()
    .value;
    assert_eq!(converted.id, "operations/real-42");
    assert_eq!(converted.polling_url, "https://proxy.test/videos/real-42");
    assert_eq!(converted.status, o::VideoStatus::Completed);
    assert_eq!(
        converted.unsigned_urls.unwrap()[0],
        "https://files.test/video.mp4"
    );
}

#[test]
fn openai_response_maps_back_without_fixed_defaults() {
    let mut response = o::VideoGenerationResponseBody::builder(
        "job-7".into(),
        "https://proxy.test/videos/job-7".into(),
        o::VideoStatus::Completed,
    )
    .build();
    response.generation_id = Some("generation-7".into());
    response.unsigned_urls = Some(vec!["https://files.test/v.mp4".into()]);
    let converted = openai_response_to_gemini_operation(
        response,
        &GeminiOperationContext {
            source_id: "job-7".into(),
            source_polling_url: "https://proxy.test/videos/job-7".into(),
            operation_name: "operations/native-7".into(),
        },
    )
    .unwrap()
    .value;
    assert_eq!(converted.name.as_deref(), Some("operations/native-7"));
    assert_eq!(converted.done, Some(true));
    assert_eq!(
        converted
            .response
            .unwrap()
            .generate_video_response
            .unwrap()
            .generated_samples
            .unwrap()[0]
            .video
            .as_ref()
            .unwrap()
            .uri
            .as_deref(),
        Some("https://files.test/v.mp4")
    );
}

#[test]
fn unsupported_requested_semantics_fail_explicitly() {
    let mut input = source();
    input.seed = Some(1);
    assert_eq!(
        openai_to_gemini_request(input, "veo", &BTreeMap::new())
            .unwrap_err()
            .kind(),
        TransformErrorKind::Unsupported
    );
    let operation: g::VideoOperation = serde_json::from_value(json!({
        "name":"operations/x","done":true,"error":{"message":"failed"}
    }))
    .unwrap();
    let response = gemini_operation_to_openai_response(
        operation,
        &OpenAiVideoResponseContext {
            operation_name: "operations/x".into(),
            polling_url: String::new(),
            resources: BTreeMap::new(),
        },
    );
    assert!(response.is_err());
}

use gproxy_protocol::gemini::video as g;

#[test]
fn exact_dimensions_provider_parameters_and_clean_context_roundtrip() {
    let input:o::CreateVideoRequestBody=serde_json::from_value(json!({"model":"google/veo","prompt":"p","size":"3840x2160","provider":{"options":{"google-ai-studio":{"negative_prompt":"blur","person_generation":"allow_adult","enhance_prompt":false,"sample_count":2}},"foreign":1},"foreign":2})).unwrap();
    let output = openai_to_gemini_request(input, "veo", &BTreeMap::new())
        .unwrap()
        .value;
    let p = output.body.parameters.as_ref().unwrap();
    assert_eq!(p.aspect_ratio.as_deref(), Some("16:9"));
    assert_eq!(p.resolution.as_deref(), Some("4K"));
    assert_eq!(p.negative_prompt.as_deref(), Some("blur"));
    assert_eq!(p.enhance_prompt, Some(false));
    assert_eq!(p.sample_count, Some(2));
    assert!(
        output
            .context
            .source_request
            .as_ref()
            .unwrap()
            .rest
            .is_empty()
    );
    assert!(
        output
            .context
            .source_request
            .as_ref()
            .unwrap()
            .provider
            .as_ref()
            .unwrap()
            .rest
            .is_empty()
    );
    let back = gemini_to_openai_request(output.body, "google/veo", &BTreeMap::new())
        .unwrap()
        .value;
    let options = back.body.provider.unwrap().options.unwrap();
    assert_eq!(options["google-ai-studio"]["negative_prompt"], "blur");
    assert_eq!(options["google-ai-studio"]["sample_count"], 2);
    let input: o::CreateVideoRequestBody =
        serde_json::from_value(json!({"model":"google/veo","prompt":"p","size":"1792x1024"}))
            .unwrap();
    assert!(openai_to_gemini_request(input, "veo", &BTreeMap::new()).is_err());
    let input:o::CreateVideoRequestBody=serde_json::from_value(json!({"model":"google/veo","prompt":"p","duration":4,"provider":{"options":{"google-ai-studio":{"duration_seconds":8}}}})).unwrap();
    assert!(openai_to_gemini_request(input, "veo", &BTreeMap::new()).is_err());
}

#[test]
fn resource_facts_cannot_alias_different_content_or_silently_overwrite_video() {
    let mut resources = BTreeMap::from([
        ("first".into(), resource("wrong")),
        ("ref".into(), resource("ref")),
    ]);
    assert!(openai_to_gemini_request(source(), "veo", &resources).is_err());
    resources.insert("first".into(), resource("first"));
    resources.get_mut("first").unwrap().mime_type = Some("video/mp4".into());
    assert!(openai_to_gemini_request(source(), "veo", &resources).is_err());
    let body: g::PredictLongRunningRequestBody = serde_json::from_value(
        json!({"instances":[{"image":{"bytesBase64Encoded":"AQI=","mimeType":"image/jpeg"}}]}),
    )
    .unwrap();
    let resources = BTreeMap::from([("AQI=".into(), resource("AQI="))]);
    assert!(gemini_to_openai_request(body, "google/veo", &resources).is_err());
    let input:o::CreateVideoRequestBody=serde_json::from_value(json!({"model":"m","input_references":[{"type":"video_url","video_url":{"url":"v1"}},{"type":"video_url","video_url":{"url":"v2"}}]})).unwrap();
    let mut first = resource("v1");
    first.mime_type = Some("video/mp4".into());
    let mut second = resource("v2");
    second.mime_type = Some("video/mp4".into());
    let resources = BTreeMap::from([("v1".into(), first), ("v2".into(), second)]);
    assert!(openai_to_gemini_request(input, "veo", &resources).is_err());
}

#[test]
fn operation_states_and_registered_polling_identity_do_not_become_fake_success() {
    let context = OpenAiVideoResponseContext {
        operation_name: "operations/x".into(),
        polling_url: "https://proxy/videos/x".into(),
        resources: BTreeMap::new(),
    };
    for value in [
        json!({"name":"operations/wrong","done":false}),
        json!({"name":"operations/x","done":true}),
        json!({"name":"operations/x","done":false,"error":{"code":3}}),
        json!({"name":"operations/x","done":true,"error":{"code":3},"response":{"generateVideoResponse":{}}}),
        json!({"name":"operations/x","done":true,"response":{"generateVideoResponse":{"generatedSamples":[{}]}}}),
    ] {
        assert!(
            gemini_operation_to_openai_response(serde_json::from_value(value).unwrap(), &context)
                .is_err()
        );
    }
    let input=serde_json::from_value(json!({"name":"operations/x","done":true,"response":{"generateVideoResponse":{"generatedSamples":[],"raiMediaFilteredCount":1,"raiMediaFilteredReasons":["safety"]}}})).unwrap();
    let output = gemini_operation_to_openai_response(input, &context)
        .unwrap()
        .value;
    assert_eq!(output.status, o::VideoStatus::Failed);
    assert_eq!(output.error.as_deref(), Some("safety"));
    let input = o::VideoGenerationResponseBody::builder(
        "source-id".into(),
        "https://source/poll".into(),
        o::VideoStatus::Completed,
    )
    .build();
    let ctx = GeminiOperationContext {
        source_id: "source-id".into(),
        source_polling_url: "https://source/poll".into(),
        operation_name: "operations/host-owned".into(),
    };
    assert!(openai_response_to_gemini_operation(input, &ctx).is_err());
}

#[test]
fn encoded_generated_video_needs_matching_publication_facts() {
    let mut published = resource("AQI=");
    published.mime_type = Some("video/mp4".into());
    let context = OpenAiVideoResponseContext {
        operation_name: "operations/x".into(),
        polling_url: "https://proxy/videos/x".into(),
        resources: BTreeMap::from([("AQI=".into(), published)]),
    };
    let input=serde_json::from_value(json!({"name":"operations/x","done":true,"response":{"generateVideoResponse":{"generatedSamples":[{"video":{"encodedVideo":"AQI=","encoding":"video/mp4"}}]}}})).unwrap();
    let output = gemini_operation_to_openai_response(input, &context)
        .unwrap()
        .value;
    assert_eq!(output.unsigned_urls.unwrap(), vec!["https://cdn.test/AQI="]);
}
