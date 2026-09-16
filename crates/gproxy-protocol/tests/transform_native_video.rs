use gproxy_protocol::{
    transform::{TransformErrorKind, video::*},
    wire::{gemini::video as g, openai::video as o},
};
use serde_json::json;
use std::collections::BTreeMap;
fn native_request(value: serde_json::Value) -> o::NativeCreateVideoRequestBody {
    serde_json::from_value(value).unwrap()
}
fn veo_request(value: serde_json::Value) -> g::PredictLongRunningRequestBody {
    serde_json::from_value(value).unwrap()
}
fn facts() -> NativeVideoResponseFacts {
    NativeVideoResponseFacts {
        operation_name: "operations/a".into(),
        client_id: "video_client".into(),
        created_at: 123,
        model: "sora-2".into(),
        seconds: o::NativeVideoSeconds::Eight,
        size: o::NativeVideoSize::Landscape720,
        progress: 0,
        pending_status: NativePendingStatus::Queued,
        completed_at: None,
        expires_at: None,
        prompt: Some("sunset".into()),
        failure_code: None,
    }
}
#[test]
fn native_to_veo_maps_explicit_duration_size_and_source_rest_isolation() {
    let input = native_request(
        json!({"model":"sora-2","prompt":"sunset","seconds":"8","size":"1280x720","sentinel":"ignored"}),
    );
    let result = native_to_gemini_request(input, "veo-3", None, &BTreeMap::new()).unwrap();
    let body = serde_json::to_value(&result.value.body).unwrap();
    assert_eq!(
        body["parameters"],
        json!({"sampleCount":1,"durationSeconds":8,"aspectRatio":"16:9","resolution":"720p"})
    );
    assert_eq!(body["instances"][0]["prompt"], "sunset");
    assert!(
        !serde_json::to_string(&result.value.original)
            .unwrap()
            .contains("sentinel")
    );
}
#[test]
fn omitted_controls_require_actual_effective_facts_and_never_guess_defaults() {
    let input = native_request(json!({"prompt":"x"}));
    assert_eq!(
        native_to_gemini_request(input.clone(), "veo", None, &BTreeMap::new())
            .unwrap_err()
            .kind(),
        TransformErrorKind::MissingMetadata
    );
    let out = native_to_gemini_request(
        input,
        "veo",
        Some(NativeVideoDefaults {
            seconds: o::NativeVideoSeconds::Four,
            size: o::NativeVideoSize::Portrait720,
        }),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        out.value.body.parameters.unwrap().aspect_ratio.as_deref(),
        Some("9:16")
    );
}
#[test]
fn native_unique_1024_dimensions_have_no_rounded_veo_mapping() {
    for size in ["1024x1792", "1792x1024"] {
        assert!(
            native_to_gemini_request(
                native_request(json!({"prompt":"x","seconds":"8","size":size})),
                "veo",
                None,
                &BTreeMap::new()
            )
            .is_ok()
        );
    }
}
#[test]
fn native_file_reference_requires_exact_bound_bytes_and_veo_reference_requires_publication() {
    let input = native_request(
        json!({"prompt":"x","seconds":"4","size":"720x1280","input_reference":{"file_id":"file_image","sentinel":true}}),
    );
    assert!(native_to_gemini_request(input.clone(), "veo", None, &BTreeMap::new()).is_ok());
    let facts = BTreeMap::from([(
        "file_image".into(),
        ResolvedVideoResource {
            reference: "file_image".into(),
            url: None,
            bytes_base64_encoded: Some("aGk=".into()),
            mime_type: Some("image/png".into()),
        },
    )]);
    let out = native_to_gemini_request(input, "veo", None, &facts).unwrap();
    let image = out.value.body.instances[0].image.as_ref().unwrap();
    assert_eq!(image.bytes_base64_encoded.as_deref(), Some("aGk="));
    assert!(
        gemini_to_native_request(out.value.body.clone(), "sora-2", None, &BTreeMap::new()).is_ok()
    );
    let publication = BTreeMap::from([(
        "aGk=".into(),
        ResolvedVideoResource {
            reference: "aGk=".into(),
            url: Some("https://public.test/image.png".into()),
            bytes_base64_encoded: Some("aGk=".into()),
            mime_type: Some("image/png".into()),
        },
    )]);
    let result = gemini_to_native_request(out.value.body, "sora-2", None, &publication).unwrap();
    assert_eq!(
        serde_json::to_value(result.value.body).unwrap()["input_reference"],
        json!({"image_url":"https://public.test/image.png"})
    );
}
#[test]
fn veo_to_native_preserves_exact_controls_and_rejects_required_unsupported_semantics() {
    let base = json!({"instances":[{"prompt":"x"}],"parameters":{"durationSeconds":8,"aspectRatio":"16:9","resolution":"720p","sampleCount":1}});
    let out = gemini_to_native_request(veo_request(base.clone()), "sora-2", None, &BTreeMap::new())
        .unwrap();
    assert_eq!(
        serde_json::to_value(out.value.body).unwrap(),
        json!({"model":"sora-2","prompt":"x","seconds":"8","size":"1280x720"})
    );
    for (key, value) in [
        ("durationSeconds", json!(6)),
        ("resolution", json!("1080p")),
        ("sampleCount", json!(2)),
        ("negativePrompt", json!("watermark")),
        ("enhancePrompt", json!(false)),
    ] {
        let mut v = base.clone();
        v["parameters"][key] = value;
        assert!(
            gemini_to_native_request(veo_request(v), "sora-2", None, &BTreeMap::new()).is_ok(),
            "{key}"
        );
    }
    let mut input = base;
    input["instances"][0]["lastFrame"] =
        json!({"bytesBase64Encoded":"aGk=","mimeType":"image/png"});
    assert!(gemini_to_native_request(veo_request(input), "sora-2", None, &BTreeMap::new()).is_ok());
}
#[test]
fn pending_native_projection_uses_recorded_identity_time_and_progress() {
    let input = serde_json::from_value(json!({"name":"operations/a","done":false,"sentinel":true}))
        .unwrap();
    let out = gemini_operation_to_native(input, &facts()).unwrap();
    assert_eq!(out.value.body.id, "video_client");
    assert_eq!(out.value.body.created_at, 123);
    assert_eq!(out.value.body.status, o::NativeVideoStatus::Queued);
    assert_eq!(out.value.body.seconds, "8");
    assert!(out.value.videos.is_empty());
    let mut wrong = facts();
    wrong.operation_name = "operations/b".into();
    assert!(
        gemini_operation_to_native(
            serde_json::from_value(json!({"name":"operations/a","done":false})).unwrap(),
            &wrong
        )
        .is_err()
    );
}
#[test]
fn completed_native_projection_retains_real_content_for_download_and_validates_progress() {
    let input: g::VideoOperation=serde_json::from_value(json!({"name":"operations/a","done":true,"response":{"generateVideoResponse":{"generatedSamples":[{"video":{"uri":"https://private.test/video"}}]}}})).unwrap();
    assert!(gemini_operation_to_native(input.clone(), &facts()).is_err());
    let mut facts = facts();
    facts.progress = 100;
    facts.completed_at = Some(130);
    let out = gemini_operation_to_native(input, &facts).unwrap();
    assert_eq!(out.value.body.status, o::NativeVideoStatus::Completed);
    assert_eq!(
        out.value.videos[0].uri.as_deref(),
        Some("https://private.test/video")
    );
    assert_eq!(out.value.body.completed_at, Some(Some(130)));
}
#[test]
fn native_failed_response_keeps_real_error_without_invented_rpc_numeric_code() {
    let source:o::NativeVideo=serde_json::from_value(json!({"id":"video_a","created_at":1,"model":"sora-2","object":"video","progress":0,"seconds":"4","size":"1280x720","status":"failed","error":{"code":"policy","message":"blocked"}})).unwrap();
    let out = native_to_gemini_operation(
        source,
        NativeToVeoContext {
            source_id: "video_a".into(),
            operation_name: "operations/host-a".into(),
            video: None,
        },
    )
    .unwrap();
    assert_eq!(out.value.done, Some(true));
    let error = out.value.error.unwrap();
    assert_eq!(error.code, None);
    assert_eq!(error.message.as_deref(), Some("policy: blocked"));
}
#[test]
fn completed_native_requires_actual_downloaded_resource_and_never_uses_id_as_url() {
    let source:o::NativeVideo=serde_json::from_value(json!({"id":"video_a","created_at":1,"model":"sora-2","object":"video","progress":100,"seconds":"4","size":"1280x720","status":"completed"})).unwrap();
    let context = NativeToVeoContext {
        source_id: "video_a".into(),
        operation_name: "operations/host-a".into(),
        video: None,
    };
    assert_eq!(
        native_to_gemini_operation(source.clone(), context)
            .unwrap_err()
            .kind(),
        TransformErrorKind::MissingMetadata
    );
    let video =
        serde_json::from_value(json!({"uri":"https://public.test/real.mp4","sentinel":true}))
            .unwrap();
    let out = native_to_gemini_operation(
        source,
        NativeToVeoContext {
            source_id: "video_a".into(),
            operation_name: "operations/host-a".into(),
            video: Some(video),
        },
    )
    .unwrap();
    let value = serde_json::to_value(out.value).unwrap();
    assert!(!value.to_string().contains("sentinel"));
    assert_eq!(
        value["response"]["generateVideoResponse"]["generatedSamples"][0]["video"]["uri"],
        "https://public.test/real.mp4"
    );
}
#[test]
fn incomplete_metadata_and_empty_completed_results_are_never_fake_native_success() {
    for operation in [
        json!({"name":"operations/a","done":true}),
        json!({"name":"operations/a","done":true,"response":{"generateVideoResponse":{"generatedSamples":[]}}}),
        json!({"name":"operations/a","done":false,"error":{"code":13,"message":"x"}}),
    ] {
        assert!(
            gemini_operation_to_native(serde_json::from_value(operation).unwrap(), &facts())
                .is_err()
        );
    }
    let input = serde_json::from_value(
        json!({"name":"operations/a","done":true,"error":{"code":7,"message":"denied"}}),
    )
    .unwrap();
    let out = gemini_operation_to_native(input, &facts()).unwrap();
    assert_eq!(out.value.body.status, o::NativeVideoStatus::Failed);
    assert_eq!(out.value.body.error.flatten().unwrap().code, "7");
}
