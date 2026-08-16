//! OpenAI create-image <-> Imagen native `:predict` conversions.

use crate::protocol::{gemini, openai};
use crate::transform::context::report_unsupported;
use crate::transform::{TransformContext, TransformError};

pub mod openai_to_gemini {
    use super::*;

    pub fn request(
        input: openai::ImageGenerationRequest,
        _: &TransformContext,
    ) -> Result<gemini::ImagenPredictRequest, TransformError> {
        if input.background.is_some() {
            report_unsupported("background", "Imagen predict has no background control");
        }
        let output_options = input.output_format.as_ref().map(|format| {
            crate::protocol::wire!(gemini::ImagenOutputOptions {
                mime_type: Some(format!("image/{}", wire_str(format))),
                compression_quality: input.output_compression,
                extra: Default::default(),
            })
        });
        Ok(crate::protocol::wire!(gemini::ImagenPredictRequest {
            instances: vec![crate::protocol::wire!(gemini::ImagenInstance {
                prompt: Some(input.prompt),
                extra: Default::default(),
            })],
            parameters: Some(crate::protocol::wire!(gemini::ImagenParameters {
                sample_count: input.n,
                aspect_ratio: input.size.as_ref().and_then(size_to_aspect),
                image_size: None,
                person_generation: None,
                negative_prompt: None,
                seed: None,
                output_options,
                extra: Default::default(),
            })),
            extra: Default::default(),
        }))
    }

    pub fn response(
        input: openai::ImagesResponse,
        _: &TransformContext,
    ) -> Result<gemini::ImagenPredictResponse, TransformError> {
        let predictions = input
            .data
            .into_iter()
            .flatten()
            .filter_map(|image| {
                if image.b64_json.is_none() {
                    report_unsupported(
                        "data[].url",
                        "Imagen predictions carry inline bytes; URL-only images cannot be converted",
                    );
                    return None;
                }
                Some(crate::protocol::wire!(gemini::ImagenPrediction {
                    bytes_base64_encoded: image.b64_json,
                    mime_type: input
                        .output_format
                        .as_ref()
                        .map(|format| format!("image/{}", wire_str(format))),
                    rai_filtered_reason: None,
                    extra: Default::default(),
                }))
            })
            .collect();
        Ok(crate::protocol::wire!(gemini::ImagenPredictResponse {
            predictions,
            extra: Default::default(),
        }))
    }
}

pub mod gemini_to_openai {
    use super::*;

    pub fn request(
        input: gemini::ImagenPredictRequest,
        _: &TransformContext,
    ) -> Result<openai::ImageGenerationRequest, TransformError> {
        let prompt = input
            .instances
            .into_iter()
            .next()
            .and_then(|instance| instance.prompt)
            .ok_or_else(|| TransformError::InvalidInput {
                reason: "Imagen request has no prompt instance".to_owned(),
            })?;
        let parameters = input.parameters.unwrap_or_default();
        if parameters.negative_prompt.is_some() {
            report_unsupported(
                "parameters.negativePrompt",
                "OpenAI image generation has no negative prompt",
            );
        }
        Ok(crate::protocol::wire!(openai::ImageGenerationRequest {
            prompt,
            background: None,
            model: None,
            moderation: None,
            n: parameters.sample_count,
            output_compression: parameters
                .output_options
                .as_ref()
                .and_then(|options| options.compression_quality),
            output_format: None,
            partial_images: None,
            quality: None,
            response_format: None,
            size: aspect_to_size(parameters.aspect_ratio.as_deref()),
            stream: None,
            style: None,
            user: None,
            extra: Default::default(),
        }))
    }

    pub fn response(
        input: gemini::ImagenPredictResponse,
        _: &TransformContext,
    ) -> Result<openai::ImagesResponse, TransformError> {
        let data = input
            .predictions
            .into_iter()
            .map(|prediction| {
                crate::protocol::wire!(openai::Image {
                    b64_json: prediction.bytes_base64_encoded,
                    revised_prompt: None,
                    url: None,
                    extra: Default::default(),
                })
            })
            .collect::<Vec<_>>();
        Ok(crate::protocol::wire!(openai::ImagesResponse {
            created: 0,
            background: None,
            data: Some(data),
            output_format: None,
            quality: None,
            size: None,
            usage: None,
            extra: Default::default(),
        }))
    }
}

fn wire_str(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// `WxH` -> Imagen aspect ratio (exact pixel dims are provider-fixed).
fn size_to_aspect(size: &openai::ImageSize) -> Option<String> {
    let value = wire_str(size);
    let (width, height) = value.split_once('x')?;
    let (width, height): (f64, f64) = (width.parse().ok()?, height.parse().ok()?);
    let ratio = width / height;
    Some(
        if ratio > 1.55 {
            "16:9"
        } else if ratio > 1.05 {
            "4:3"
        } else if ratio > 0.95 {
            "1:1"
        } else if ratio > 0.65 {
            "3:4"
        } else {
            "9:16"
        }
        .to_owned(),
    )
}

fn aspect_to_size(aspect: Option<&str>) -> Option<openai::ImageSize> {
    let value = match aspect? {
        "1:1" => "1024x1024",
        "16:9" | "4:3" => "1536x1024",
        "9:16" | "3:4" => "1024x1536",
        _ => return None,
    };
    Some(
        serde_json::from_value(serde_json::json!(value))
            .unwrap_or(openai::ImageSize::Unknown(value.to_owned())),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{Operation, OperationKey, Provider};

    /// 尺寸→宽高比与 predictions→data 是仅有的非平凡映射。
    #[test]
    fn maps_native_imagen_request_and_response() {
        let ctx = TransformContext::new(
            OperationKey::provider(Operation::CreateImage, Provider::OpenAi),
            OperationKey::provider(Operation::CreateImage, Provider::Gemini),
        );
        let request: openai::ImageGenerationRequest = serde_json::from_value(json!({
            "prompt": "远山水墨",
            "model": "imagen-4.0-generate-001",
            "n": 2,
            "size": "1024x1536"
        }))
        .unwrap();
        let predict = openai_to_gemini::request(request, &ctx).unwrap();
        let value = serde_json::to_value(&predict).unwrap();
        assert_eq!(value["instances"][0]["prompt"], "远山水墨");
        assert_eq!(value["parameters"]["sampleCount"], 2);
        assert_eq!(value["parameters"]["aspectRatio"], "3:4");

        let response: gemini::ImagenPredictResponse = serde_json::from_value(json!({
            "predictions": [{"bytesBase64Encoded": "AAAA", "mimeType": "image/png"}]
        }))
        .unwrap();
        let images = gemini_to_openai::response(response, &ctx).unwrap();
        assert_eq!(images.data.as_ref().unwrap().len(), 1);
        assert_eq!(images.data.unwrap()[0].b64_json.as_deref(), Some("AAAA"));
    }
}
