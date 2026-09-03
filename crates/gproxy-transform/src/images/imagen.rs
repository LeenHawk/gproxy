use bytes::Bytes;
use gproxy_protocol::gemini;
use gproxy_protocol::openai::images as openai_images;

use crate::TransformError;

pub(crate) fn openai_request(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai_images::CreateImageRequest = serde_json::from_slice(&body)?;
    super::encode(&openai_request_typed(input))
}

pub(crate) fn openai_request_typed(
    input: openai_images::CreateImageRequest,
) -> gemini::ImagenPredictRequest {
    let parameters = serde_json::json!({
        "sampleCount": input.n.unwrap_or(1),
        "outputOptions": {
            "mimeType": input.output_format.as_ref().and_then(wire).map(|v| format!("image/{v}"))
        },
        "aspectRatio": input.size.as_ref().and_then(wire)
    });
    crate::wire!(gemini::ImagenPredictRequest {
        instances: vec![serde_json::json!({"prompt": input.prompt})],
        parameters: Some(parameters),
        rest: Default::default(),
    })
}

pub(crate) fn gemini_request(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: gemini::ImagenPredictRequest = serde_json::from_slice(&body)?;
    super::encode(&gemini_request_typed(input, model))
}

pub(crate) fn gemini_request_typed(
    input: gemini::ImagenPredictRequest,
    model: &str,
) -> openai_images::CreateImageRequest {
    let prompt = input
        .instances
        .iter()
        .find_map(|instance| instance.get("prompt").and_then(serde_json::Value::as_str))
        .unwrap_or_default()
        .to_owned();
    let n = input
        .parameters
        .as_ref()
        .and_then(|value| value.get("sampleCount"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    crate::wire!(openai_images::CreateImageRequest {
        prompt,
        model: Some(model.into()),
        n,
        rest: Default::default(),
        background: None,
        moderation: None,
        output_compression: None,
        output_format: None,
        partial_images: None,
        quality: None,
        response_format: None,
        size: None,
        stream: None,
        style: None,
        user: None,
    })
}

pub(crate) fn gemini_response_to_openai(body: Bytes) -> Result<Bytes, TransformError> {
    let input: gemini::ImagenPredictResponse = serde_json::from_slice(&body)?;
    super::encode(&gemini_response_to_openai_typed(input))
}

pub(crate) fn gemini_response_to_openai_typed(
    input: gemini::ImagenPredictResponse,
) -> openai_images::ImagesResponse {
    let data = input
        .predictions
        .into_iter()
        .filter_map(|prediction| {
            let b64_json = prediction
                .get("bytesBase64Encoded")
                .or_else(|| prediction.pointer("/image/bytesBase64Encoded"))
                .and_then(serde_json::Value::as_str)?
                .to_owned();
            Some(crate::wire!(openai_images::Image {
                b64_json: Some(b64_json),
                revised_prompt: prediction
                    .get("prompt")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                url: None,
                rest: Default::default(),
            }))
        })
        .collect();
    crate::wire!(openai_images::ImagesResponse {
        created: 0,
        data: Some(data),
        rest: Default::default(),
        background: None,
        output_format: None,
        quality: None,
        size: None,
        usage: None,
    })
}

pub(crate) fn openai_response_to_gemini(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai_images::ImagesResponse = serde_json::from_slice(&body)?;
    super::encode(&openai_response_to_gemini_typed(input))
}

pub(crate) fn openai_response_to_gemini_typed(
    input: openai_images::ImagesResponse,
) -> gemini::ImagenPredictResponse {
    let predictions = input
        .data
        .unwrap_or_default()
        .into_iter()
        .filter_map(|image| {
            image.b64_json.map(|data| {
                serde_json::json!({
                    "bytesBase64Encoded": data,
                    "prompt": image.revised_prompt
                })
            })
        })
        .collect();
    crate::wire!(gemini::ImagenPredictResponse {
        predictions,
        rest: Default::default(),
    })
}

fn wire(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()?
        .as_str()
        .map(str::to_owned)
}
