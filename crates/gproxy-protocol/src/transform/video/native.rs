//! Native Sora JSON request mapping. Native sizes/durations are not the
//! OpenRouter contract and do not pass through its request DTO.

use super::ResolvedVideoResource;
use crate::{
    transform::{Converted, Report, TransformError},
    wire::{DeclaredFields, gemini::video as g, openai::video as o},
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeVideoDefaults {
    /// Effective source settings, supplied from the selected source model's
    /// invocation contract when the client omitted these controls.
    pub seconds: o::NativeVideoSeconds,
    pub size: o::NativeVideoSize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedNativeVeoRequest {
    pub body: g::PredictLongRunningRequestBody,
    pub original: o::NativeCreateVideoRequestBody,
    pub effective: NativeVideoDefaults,
    pub target_model: String,
}

pub fn native_to_gemini_request(
    input: o::NativeCreateVideoRequestBody,
    target_model: &str,
    defaults: Option<NativeVideoDefaults>,
    resources: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<Converted<PreparedNativeVeoRequest>, TransformError> {
    let input = input.into_declared();

    let seconds = input
        .seconds
        .or(defaults.as_ref().map(|v| v.seconds))
        .ok_or_else(|| TransformError::missing_metadata("native.effective_seconds"))?;
    let size = input
        .size
        .or(defaults.as_ref().map(|v| v.size))
        .ok_or_else(|| TransformError::missing_metadata("native.effective_size"))?;
    let aspect = match size {
        o::NativeVideoSize::Portrait720 => Some("9:16"),
        o::NativeVideoSize::Landscape720 => Some("16:9"),
        _ => None,
    };
    let seconds_value = match seconds {
        o::NativeVideoSeconds::Four => 4,
        o::NativeVideoSeconds::Eight => 8,
        o::NativeVideoSeconds::Twelve => 12,
    };
    let image = input
        .input_reference
        .as_ref()
        .map(|reference| {
            let key = match reference {
                o::NativeInputReference::File(v) => &v.file_id,
                o::NativeInputReference::Url(v) => &v.image_url,
            };
            super::resources::image_resource(key, resources, super::VideoResourceRole::FirstFrame)
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let mut instance = g::VideoGenerationInstance::builder()
        .prompt(input.prompt.clone())
        .build();
    instance.image = image;
    let mut parameters = g::VideoGenerationParameters::builder()
        .sample_count(1)
        .duration_seconds(seconds_value)
        .build();
    parameters.aspect_ratio = aspect.map(str::to_owned);
    parameters.resolution = aspect.map(|_| "720p".to_owned());
    let body = g::PredictLongRunningRequestBody::builder(vec![instance])
        .parameters(parameters)
        .build();
    Ok(Converted {
        value: PreparedNativeVeoRequest {
            body,
            original: input,
            effective: NativeVideoDefaults { seconds, size },
            target_model: target_model.into(),
        },
        report: Report::default(),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedVeoNativeRequest {
    pub body: o::NativeCreateVideoRequestBody,
    pub original: g::PredictLongRunningRequestBody,
    pub target_model: String,
}

/// One instance/sample; multi-instance or sample fanout belongs to the composed
/// adapter. The return projection still belongs to the original Veo request.
pub fn gemini_to_native_request(
    input: g::PredictLongRunningRequestBody,
    target_model: &str,
    defaults: Option<NativeVideoDefaults>,
    resources: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<Converted<PreparedVeoNativeRequest>, TransformError> {
    let input = input.into_declared();
    if target_model.trim().is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }

    let instance = input
        .instances
        .first()
        .ok_or_else(|| TransformError::missing_metadata("video.instance"))?;

    let prompt = instance
        .prompt
        .as_ref()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| TransformError::missing_metadata("native.prompt"))?
        .clone();
    let parameters = input.parameters.as_ref();

    let seconds = match parameters.and_then(|p| p.duration_seconds) {
        Some(4) => Some(o::NativeVideoSeconds::Four),
        Some(8) => Some(o::NativeVideoSeconds::Eight),
        Some(12) => Some(o::NativeVideoSeconds::Twelve),
        Some(_) => None,
        None => defaults.as_ref().map(|v| v.seconds),
    };
    let aspect = parameters.and_then(|p| p.aspect_ratio.as_deref());
    let resolution = parameters.and_then(|p| p.resolution.as_deref());
    let size = match (aspect, resolution) {
        (Some("16:9"), Some("720p")) => Some(o::NativeVideoSize::Landscape720),
        (Some("9:16"), Some("720p")) => Some(o::NativeVideoSize::Portrait720),
        (None, None) => defaults.as_ref().map(|v| v.size),
        _ => None,
    };
    let reference = instance
        .image
        .as_ref()
        .map(|image| {
            super::resources::published_url(image, resources).map(|url| {
                o::NativeInputReference::Url(o::NativeUrlReference::builder(url).build())
            })
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let mut body = o::NativeCreateVideoRequestBody::builder(prompt)
        .model(target_model)
        .build();
    body.seconds = seconds;
    body.size = size;
    body.input_reference = reference;
    Ok(Converted {
        value: PreparedVeoNativeRequest {
            body,
            original: input,
            target_model: target_model.into(),
        },
        report: Report::default(),
    })
}
