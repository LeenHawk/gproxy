use gproxy_protocol::openai::video::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn round<T: Serialize + DeserializeOwned>(v: Value) -> T {
    let p: T = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
    p
}
fn extended() -> Value {
    json!({"model":"google/veo-3.1","prompt":"sunset","duration":6,"aspect_ratio":"16:9","resolution":"4K","size":"3840x2160","seed":1,"frame_images":[{"type":"image_url","image_url":{"url":"https://first"},"frame_type":"first_frame"},{"type":"image_url","image_url":{"url":"https://last"},"frame_type":"last_frame"}],"input_references":[{"type":"image_url","image_url":{"url":"https://image"}},{"type":"audio_url","audio_url":{"url":"https://audio"}},{"type":"video_url","video_url":{"url":"https://video"}}],"generate_audio":true,"callback_url":"https://callback","provider":{"options":{"byteplus":{"future_mode":{"x":1}},"google-vertex":{"output_config":{"effort":"low"}}}}})
}
fn native() -> Value {
    json!({"id":"v","created_at":123,"model":"sora-future","object":"video","progress":100,"seconds":"20","size":"1280x720","status":"completed","completed_at":124,"error":{"code":"blocked","message":"error","misalignment":{"detailed_explanation":null,"error_type":"future-category","steer":{"message":"try something else"}}},"expires_at":200,"prompt":null,"remixed_from_video_id":null})
}
#[test]
fn extended_all_twelve_request_fields_and_reference_unions_are_typed() {
    let p = round::<CreateVideoRequestBody>(extended());
    assert!(p.rest.is_empty());
    assert_eq!(p.duration, Some(6));
    let provider = p.provider.unwrap();
    assert!(provider.rest.is_empty());
    assert_eq!(
        provider.options.unwrap()["byteplus"]["future_mode"],
        json!({"x":1})
    );
    for f in p.frame_images.unwrap() {
        assert!(f.rest.is_empty());
        assert!(f.image_url.rest.is_empty());
    }
    for r in p.input_references.unwrap() {
        match r {
            InputReference::Image(v) => {
                assert!(v.rest.is_empty());
                assert!(v.image_url.rest.is_empty());
            }
            InputReference::Audio(v) => {
                assert!(v.rest.is_empty());
                assert!(v.audio_url.rest.is_empty());
            }
            InputReference::Video(v) => {
                assert!(v.rest.is_empty());
                assert!(v.video_url.rest.is_empty());
            }
            #[cfg(not(feature = "exhaustive"))]
            _ => panic!(),
        }
    }
    assert_eq!(extended().as_object().unwrap().len(), 12);
    let p = round::<CreateVideoRequestBody>(json!({"model":"m"}));
    assert!(p.prompt.is_none());
    assert!(serde_json::from_value::<CreateVideoRequestBody>(json!({"prompt":"x"})).is_err());
    let base = extended();
    for field in base.as_object().unwrap().keys() {
        let mut v = base.clone();
        v[field] = Value::Null;
        assert!(
            serde_json::from_value::<CreateVideoRequestBody>(v).is_err(),
            "null {field}"
        );
        if field != "model" {
            let mut v = base.clone();
            v.as_object_mut().unwrap().remove(field);
            round::<CreateVideoRequestBody>(v);
        }
    }
    round::<VideoProvider>(json!({}));
    assert!(serde_json::from_value::<VideoProvider>(json!({"options":{"x":true}})).is_err());
    assert!(
        serde_json::from_value::<InputReference>(json!({"type":"Image","image_url":{"url":"u"}}))
            .is_err()
    );
}
#[test]
fn submit_poll_responses_use_only_openrouter_job_shape() {
    // Exact local Submit example, also the Poll VideoGenerationResponse example.
    let p = round::<VideoGenerationResponseBody>(
        json!({"generation_id":"gen-xyz789","id":"job-abc123","polling_url":"/api/v1/videos/job-abc123","status":"pending"}),
    );
    assert!(p.rest.is_empty());
    let full = json!({"id":"job","polling_url":"/api/v1/videos/job","status":"failed","error":"provider failed","generation_id":"gen","unsigned_urls":["https://video"],"usage":{"cost":null,"is_byok":false}});
    let p = round::<VideoGenerationResponseBody>(full.clone());
    assert!(p.rest.is_empty());
    assert!(p.usage.unwrap().rest.is_empty());
    for field in ["id", "polling_url", "status"] {
        let mut v = full.clone();
        v.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<VideoGenerationResponseBody>(v).is_err());
    }
    for field in ["error", "generation_id", "unsigned_urls", "usage"] {
        let mut v = full.clone();
        v.as_object_mut().unwrap().remove(field);
        round::<VideoGenerationResponseBody>(v.clone());
        v[field] = Value::Null;
        assert!(serde_json::from_value::<VideoGenerationResponseBody>(v).is_err());
    }
    for usage in [
        json!({}),
        json!({"cost":null}),
        json!({"cost":0.5,"is_byok":true}),
    ] {
        round::<VideoUsage>(usage);
    }
    assert!(serde_json::from_value::<VideoUsage>(json!({"is_byok":null})).is_err());
    assert!(serde_json::from_value::<VideoGenerationResponseBody>(native()).is_err());
}
#[test]
fn native_create_resource_and_crud_are_separate_exact_contracts() {
    let p = round::<NativeCreateVideoRequestBody>(
        json!({"prompt":"x","input_reference":{"file_id":"f"},"model":"sora-future","seconds":"8","size":"720x1280"}),
    );
    assert!(p.rest.is_empty());
    let p = round::<NativeVideo>(native());
    assert!(p.rest.is_empty());
    assert_eq!(p.seconds, "20");
    let err = p.error.unwrap().unwrap();
    assert!(err.rest.is_empty());
    let misalignment = err.misalignment.unwrap().unwrap();
    assert!(misalignment.rest.is_empty());
    assert!(misalignment.steer.unwrap().unwrap().rest.is_empty());
    let base = native();
    for field in [
        "id",
        "created_at",
        "model",
        "object",
        "progress",
        "seconds",
        "size",
        "status",
    ] {
        let mut v = base.clone();
        v.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<NativeVideo>(v).is_err(),
            "missing {field}"
        );
    }
    for field in [
        "completed_at",
        "error",
        "expires_at",
        "prompt",
        "remixed_from_video_id",
    ] {
        let mut v = base.clone();
        v.as_object_mut().unwrap().remove(field);
        round::<NativeVideo>(v.clone());
        v[field] = Value::Null;
        round::<NativeVideo>(v);
    }
    let p = round::<NativeListVideosQuery>(json!({"after":"v","limit":10,"order":"desc"}));
    assert!(p.rest.is_empty());
    let p = round::<NativeListVideosResponseBody>(
        json!({"data":[native()],"has_more":false,"last_id":"v"}),
    );
    assert!(p.rest.is_empty());
    assert!(p.data[0].rest.is_empty());
    round::<NativeListVideosResponseBody>(json!({"data":[]}));
    round::<NativeListVideosResponseBody>(json!({"data":[],"has_more":null,"last_id":null}));
    let p = round::<NativeDeleteVideoResponseBody>(
        json!({"id":"v","deleted":true,"object":"video.deleted"}),
    );
    assert!(p.rest.is_empty());
    assert!(serde_json::from_value::<NativeCreateVideoRequestBody>(json!({"model":"m"})).is_err());
    assert!(
        serde_json::from_value::<NativeCreateVideoRequestBody>(
            json!({"prompt":"x","seconds":"20"})
        )
        .is_err()
    );
}
#[test]
fn native_reference_exactly_one_field_does_not_erase_extensions() {
    for v in [
        json!({"file_id":"f","future":true}),
        json!({"image_url":"https://x","future":true}),
    ] {
        round::<NativeInputReference>(v);
    }
    for v in [
        json!({}),
        json!({"file_id":"f","image_url":"https://x"}),
        json!({"file_id":"f","image_url":null}),
        json!({"file_id":null,"image_url":"https://x"}),
        json!("file-id"),
    ] {
        assert!(serde_json::from_value::<NativeInputReference>(v).is_err());
    }
}
#[test]
fn no_implicit_native_extension_conversion_or_download_schema_mixing() {
    let p = round::<CreateVideoRequestBody>(
        json!({"model":"m","seconds":"8","input_reference":{"file_id":"f"}}),
    );
    assert!(p.duration.is_none());
    assert!(p.input_references.is_none());
    assert_eq!(p.rest.len(), 2);
    let p = round::<NativeCreateVideoRequestBody>(
        json!({"prompt":"x","duration":6,"input_references":[]}),
    );
    assert!(p.seconds.is_none());
    assert!(p.input_reference.is_none());
    assert_eq!(p.rest.len(), 2);
    let p = round::<DownloadVideoQuery>(json!({"index":null,"variant":"thumbnail"}));
    assert_eq!(p.index, Some(None));
    assert!(p.rest.contains_key("variant"));
    let p = round::<NativeDownloadVideoQuery>(json!({"variant":"thumbnail","index":1}));
    assert!(p.rest.contains_key("index"));
    assert!(serde_json::from_value::<NativeDownloadVideoQuery>(json!({"variant":null})).is_err());
    let p = round::<CreateVideoRequestBody>(json!({"model":"m","aspectRatio":"16:9"}));
    assert!(p.aspect_ratio.is_none());
    assert!(p.rest.contains_key("aspectRatio"));
}
fn closed<T: Serialize + DeserializeOwned>(values: &[&str]) {
    for v in values {
        round::<T>(json!(v));
    }
    assert!(serde_json::from_value::<T>(json!("unknown")).is_err());
}
#[test]
fn each_status_resolution_aspect_and_native_enum_is_source_exact() {
    closed::<VideoStatus>(&[
        "pending",
        "in_progress",
        "completed",
        "failed",
        "cancelled",
        "expired",
    ]);
    closed::<NativeVideoStatus>(&["queued", "in_progress", "completed", "failed"]);
    assert!(serde_json::from_value::<VideoStatus>(json!("queued")).is_err());
    assert!(serde_json::from_value::<NativeVideoStatus>(json!("pending")).is_err());
    closed::<VideoAspectRatio>(&[
        "16:9", "9:16", "1:1", "4:3", "3:4", "3:2", "2:3", "21:9", "9:21",
    ]);
    closed::<VideoResolution>(&["480p", "720p", "768p", "1080p", "1K", "2K", "4K"]);
    closed::<FrameType>(&["first_frame", "last_frame"]);
    closed::<NativeVideoSeconds>(&["4", "8", "12"]);
    closed::<NativeVideoSize>(&["720x1280", "1280x720", "1024x1792", "1792x1024"]);
    closed::<NativeVideoDownloadVariant>(&["video", "thumbnail", "spritesheet"]);
    closed::<NativeVideoListOrder>(&["asc", "desc"]);
}
#[test]
fn typed_builders_and_http_streams_retain_metadata_and_real_file_parts() {
    use gproxy_protocol::connection::{HttpBody, MultipartPart};
    struct Bytes;
    impl futures_core::Stream for Bytes {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(None)
        }
    }
    let ext = CreateVideoRequestBody::builder("m".into())
        .duration(6)
        .resolution(VideoResolution::K4)
        .build();
    assert_eq!(
        serde_json::to_value(&ext).unwrap(),
        json!({"model":"m","duration":6,"resolution":"4K"})
    );
    let request: CreateVideoRequest = gproxy_protocol::WireRequest {
        method: http::Method::POST,
        path: "/videos".into(),
        query: Some("x=1".into()),
        headers: http::HeaderMap::new(),
        body: ext,
    };
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.path, "/videos");
    assert_eq!(request.query.as_deref(), Some("x=1"));
    assert!(request.headers.is_empty());
    assert_eq!(request.body.duration, Some(6));
    let part = MultipartPart {
        headers: http::HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(Bytes)),
    };
    let form = NativeCreateVideoMultipartForm::builder("x".into())
        .input_reference(part)
        .seconds(NativeVideoSeconds::Eight)
        .size(NativeVideoSize::Landscape720)
        .build();
    assert!(matches!(
        form.input_reference.unwrap().body,
        HttpBody::Stream(_)
    ));
    let native = NativeCreateVideoRequestBody::builder("x".into())
        .input_reference(NativeInputReference::File(
            NativeFileReference::builder("f".into()).build(),
        ))
        .seconds(NativeVideoSeconds::Four)
        .build();
    assert_eq!(
        serde_json::to_value(native).unwrap(),
        json!({"prompt":"x","input_reference":{"file_id":"f"},"seconds":"4"})
    );
    let list: NativeListVideosRequest = gproxy_protocol::WireRequest {
        method: http::Method::GET,
        path: "/videos".into(),
        query: None,
        headers: http::HeaderMap::new(),
        body: (),
    };
    assert_eq!(list.body, ());
    let response: CreateVideoResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::ACCEPTED,
        headers: http::HeaderMap::new(),
        body: VideoGenerationResponseBody::builder(
            "j".into(),
            "/videos/j".into(),
            VideoStatus::Pending,
        )
        .build(),
    };
    assert_eq!(response.status, http::StatusCode::ACCEPTED);
    assert!(response.headers.is_empty());
    assert_eq!(
        serde_json::to_value(response.body).unwrap(),
        json!({"id":"j","polling_url":"/videos/j","status":"pending"})
    );
    let download: DownloadVideoResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(Bytes)),
    };
    assert!(matches!(download.body, HttpBody::Stream(_)));
}

#[test]
fn path_parameter_spelling_is_native_to_each_source() {
    let p = round::<RetrieveVideoPath>(json!({"jobId":"job"}));
    assert_eq!(p.job_id, "job");
    assert!(p.rest.is_empty());
    let p = round::<NativeVideoPath>(json!({"video_id":"video"}));
    assert_eq!(p.video_id, "video");
    assert!(p.rest.is_empty());
}
