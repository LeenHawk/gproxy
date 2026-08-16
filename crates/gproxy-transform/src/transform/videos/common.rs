use crate::protocol::{gemini, openai};
use crate::transform::context::report_lossy;

/// Gemini Veo long-running operation -> OpenAI video job object.
pub fn operation_to_video(input: gemini::VeoOperation) -> openai::Video {
    let model = input
        .name
        .strip_prefix("models/")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("veo")
        .to_owned();
    let (status, error, progress) = if let Some(error) = input.error {
        (
            openai::VideoStatus::Known(openai::VideoStatusKnown::Failed),
            Some(crate::protocol::wire!(openai::VideoCreateError {
                code: error
                    .code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "failed".to_owned()),
                message: error
                    .message
                    .unwrap_or_else(|| "video generation failed".to_owned()),
                extra: Default::default(),
            })),
            100,
        )
    } else if input.done {
        (
            openai::VideoStatus::Known(openai::VideoStatusKnown::Completed),
            None,
            100,
        )
    } else {
        (
            openai::VideoStatus::Known(openai::VideoStatusKnown::InProgress),
            None,
            0,
        )
    };
    let uri = input
        .response
        .as_ref()
        .and_then(|response| response.generate_video_response.as_ref())
        .and_then(|response| {
            response
                .generated_samples
                .iter()
                .filter_map(|sample| sample.video.as_ref())
                .chain(response.videos.iter())
                .find_map(|video| video.uri.clone().or_else(|| video.gcs_uri.clone()))
        });
    let mut extra = openai::Extra::new();
    if let Some(uri) = uri {
        // OpenAI's shape has no field for a result reference; downloads use a
        // separate endpoint. Surface the Veo file URI for the caller.
        extra.insert("gproxy_video_uri".to_owned(), serde_json::json!(uri));
    }
    crate::protocol::wire!(openai::Video {
        id: input.name,
        completed_at: None,
        created_at: 0,
        error,
        expires_at: None,
        model: openai::VideoModelId::Unknown(model),
        object: openai::VideoObjectType::Video,
        progress,
        prompt: None,
        remixed_from_video_id: None,
        seconds: "0".to_owned(),
        size: openai::VideoSize::Unknown("unspecified".to_owned()),
        status,
        extra,
    })
}

/// OpenAI video job object -> Gemini Veo long-running operation.
pub fn video_to_operation(input: openai::Video) -> gemini::VeoOperation {
    let done = matches!(
        &input.status,
        openai::VideoStatus::Known(
            openai::VideoStatusKnown::Completed | openai::VideoStatusKnown::Failed
        )
    );
    if done {
        report_lossy(
            "video.response",
            "OpenAI serves video bytes from a separate content endpoint; the converted operation carries no video reference",
        );
    }
    let error = input.error.map(|error| {
        crate::protocol::wire!(gemini::VeoOperationError {
            code: None,
            message: Some(error.message),
            extra: Default::default(),
        })
    });
    crate::protocol::wire!(gemini::VeoOperation {
        name: input.id,
        metadata: None,
        done,
        error,
        response: None,
        extra: Default::default(),
    })
}

/// `WxH` -> Veo aspect ratio and resolution (exact pixel dims are lossy).
pub(super) fn size_to_veo(size: &openai::VideoSize) -> (Option<String>, Option<String>) {
    let value = serde_json::to_value(size)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned));
    let Some((width, height)) = value.as_deref().and_then(parse_size) else {
        return (None, None);
    };
    let aspect = if width >= height { "16:9" } else { "9:16" };
    let resolution = if width.min(height) >= 1080 {
        "1080p"
    } else {
        "720p"
    };
    (Some(aspect.to_owned()), Some(resolution.to_owned()))
}

pub(super) fn veo_to_size(aspect: Option<&str>, resolution: Option<&str>) -> openai::VideoSize {
    let portrait = aspect == Some("9:16");
    let high = resolution == Some("1080p");
    let value = match (portrait, high) {
        (false, false) => "1280x720",
        (true, false) => "720x1280",
        (false, true) => "1792x1024",
        (true, true) => "1024x1792",
    };
    serde_json::from_value(serde_json::json!(value))
        .unwrap_or(openai::VideoSize::Unknown(value.to_owned()))
}

fn parse_size(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}
