use bytes::Bytes;
use gproxy_protocol::gemini;
use gproxy_protocol::openai::video as openai_video;

use crate::TransformError;

pub(crate) fn openai_request(body: Bytes) -> Result<Bytes, TransformError> {
    if body.is_empty() {
        return Ok(body);
    }
    let input: openai_video::CreateVideoRequest = serde_json::from_slice(&body)?;
    let image = input.input_reference.and_then(reference_value);
    let mut instance = serde_json::json!({"prompt": input.prompt});
    if let Some(image) = image {
        instance["image"] = image;
    }
    let parameters = serde_json::json!({
        "durationSeconds": input.seconds.as_ref().and_then(wire).and_then(|value| value.parse::<u64>().ok()),
        "aspectRatio": input.size.as_ref().and_then(wire).as_deref().and_then(aspect_ratio),
        "resolution": input.size.as_ref().and_then(wire).as_deref().and_then(resolution)
    });
    super_encode(&gemini::VeoPredictLongRunningRequest {
        instances: vec![instance],
        parameters: Some(parameters),
        rest: input.rest,
    })
}

pub(crate) fn gemini_request(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    if body.is_empty() {
        return Ok(body);
    }
    let input: gemini::VeoPredictLongRunningRequest = serde_json::from_slice(&body)?;
    let instance = input.instances.first().cloned().unwrap_or_default();
    let prompt = instance
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let parameters = input.parameters.unwrap_or_default();
    let seconds = parameters
        .get("durationSeconds")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| serde_json::from_value(serde_json::json!(value.to_string())).ok());
    let size = veo_size(
        parameters
            .get("aspectRatio")
            .and_then(serde_json::Value::as_str),
        parameters
            .get("resolution")
            .and_then(serde_json::Value::as_str),
    );
    super_encode(&openai_video::CreateVideoRequest {
        prompt,
        input_reference: None,
        model: Some(openai_video::VideoModelId::Unknown(model.into())),
        seconds,
        size: serde_json::from_value(serde_json::json!(size)).ok(),
        rest: input.rest,
    })
}

pub(crate) fn gemini_response_to_openai(body: Bytes) -> Result<Bytes, TransformError> {
    let input: gemini::VeoOperation = serde_json::from_slice(&body)?;
    let name = input.name.clone().unwrap_or_else(|| "operation".into());
    let failed = input.error.is_some();
    let done = input.done.unwrap_or(false);
    let status = if failed {
        openai_video::VideoStatus::Known(openai_video::KnownVideoStatus::Failed)
    } else if done {
        openai_video::VideoStatus::Known(openai_video::KnownVideoStatus::Completed)
    } else {
        openai_video::VideoStatus::Known(openai_video::KnownVideoStatus::InProgress)
    };
    let error = input.error.map(|error| openai_video::VideoError {
        code: error
            .code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "failed".into()),
        message: error
            .message
            .unwrap_or_else(|| "video generation failed".into()),
        rest: error.rest,
    });
    let mut rest = input.rest;
    if let Some(response) = input.response {
        rest.insert("gproxy_veo_response".into(), response);
    }
    let model = name
        .strip_prefix("models/")
        .and_then(|value| value.split('/').next())
        .unwrap_or("veo")
        .to_owned();
    super_encode(&openai_video::Video {
        id: name,
        completed_at: None,
        created_at: 0,
        error,
        expires_at: None,
        model: openai_video::VideoModelId::Unknown(model),
        object: openai_video::VideoObjectType::Known(openai_video::KnownVideoObjectType::Video),
        progress: if done || failed { 100 } else { 0 },
        prompt: None,
        remixed_from_video_id: None,
        seconds: openai_video::VideoSecondsValue::String("0".into()),
        size: serde_json::from_value(serde_json::json!("1280x720"))?,
        status,
        rest,
    })
}

pub(crate) fn openai_response_to_gemini(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai_video::Video = serde_json::from_slice(&body)?;
    let done = matches!(
        input.status,
        openai_video::VideoStatus::Known(
            openai_video::KnownVideoStatus::Completed | openai_video::KnownVideoStatus::Failed
        )
    );
    let error = input.error.map(|error| gemini::Status {
        code: error.code.parse().ok(),
        message: Some(error.message),
        details: Vec::new(),
        rest: error.rest,
    });
    super_encode(&gemini::VeoOperation {
        name: Some(input.id),
        metadata: None,
        done: Some(done),
        error,
        response: input.rest.get("gproxy_veo_response").cloned(),
        rest: input.rest,
    })
}

fn reference_value(reference: openai_video::VideoInputReference) -> Option<serde_json::Value> {
    match reference {
        openai_video::VideoInputReference::File(value) => data_url(&value),
        openai_video::VideoInputReference::Image(image) => {
            image.image_url.as_deref().and_then(data_url)
        }
        openai_video::VideoInputReference::Raw(_) => None,
    }
}

fn data_url(value: &str) -> Option<serde_json::Value> {
    let value = value.strip_prefix("data:")?;
    let (mime_type, data) = value.split_once(";base64,")?;
    Some(serde_json::json!({"bytesBase64Encoded": data, "mimeType": mime_type}))
}

fn aspect_ratio(size: &str) -> Option<&'static str> {
    let (width, height) = dimensions(size)?;
    Some(if width >= height { "16:9" } else { "9:16" })
}

fn resolution(size: &str) -> Option<&'static str> {
    let (width, height) = dimensions(size)?;
    Some(if width.min(height) >= 1080 {
        "1080p"
    } else {
        "720p"
    })
}

fn dimensions(size: &str) -> Option<(u32, u32)> {
    let (width, height) = size.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

fn veo_size(aspect: Option<&str>, resolution: Option<&str>) -> &'static str {
    match (aspect == Some("9:16"), resolution == Some("1080p")) {
        (false, false) => "1280x720",
        (true, false) => "720x1280",
        (false, true) => "1792x1024",
        (true, true) => "1024x1792",
    }
}

fn wire(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()?
        .as_str()
        .map(str::to_owned)
}

fn super_encode(value: &impl serde::Serialize) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}
