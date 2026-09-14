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
    if input.prompt.trim().is_empty() || target_model.trim().is_empty() {
        return Err(TransformError::shape(
            "video.prompt/model",
            "nonempty prompt and selected target model required",
        ));
    }
    let seconds = input
        .seconds
        .or(defaults.as_ref().map(|v| v.seconds))
        .ok_or_else(|| TransformError::missing_metadata("native.effective_seconds"))?;
    let size = input
        .size
        .or(defaults.as_ref().map(|v| v.size))
        .ok_or_else(|| TransformError::missing_metadata("native.effective_size"))?;
    let aspect = match size {
        o::NativeVideoSize::Portrait720 => "9:16",
        o::NativeVideoSize::Landscape720 => "16:9",
        o::NativeVideoSize::Portrait1024 | o::NativeVideoSize::Landscape1024 => {
            return Err(TransformError::unsupported(
                "native.size",
                "Veo has no exact 1024x1792 or 1792x1024 dimensions",
            ));
        }
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
        .transpose()?;
    let mut instance = g::VideoGenerationInstance::builder()
        .prompt(input.prompt.clone())
        .build();
    instance.image = image;
    let body = g::PredictLongRunningRequestBody::builder(vec![instance])
        .parameters(
            g::VideoGenerationParameters::builder()
                .sample_count(1)
                .duration_seconds(seconds_value)
                .aspect_ratio(aspect)
                .resolution("720p")
                .build(),
        )
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
    if input.instances.len() != 1 {
        return Err(TransformError::unsupported(
            "instances",
            "native video create requires instance fanout",
        ));
    }
    if input
        .webhook_config
        .as_ref()
        .is_some_and(|v| v.uris.is_some() || v.user_metadata.is_some())
    {
        return Err(TransformError::unsupported(
            "webhookConfig",
            "native Sora has no webhook relay contract",
        ));
    }
    let instance = &input.instances[0];
    if instance.video.is_some()
        || instance.last_frame.is_some()
        || instance
            .reference_images
            .as_ref()
            .is_some_and(|v| !v.is_empty())
    {
        return Err(TransformError::unsupported(
            "instance.conditioning",
            "native Sora accepts a first-frame image only",
        ));
    }
    let prompt = instance
        .prompt
        .as_ref()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| TransformError::missing_metadata("native.prompt"))?
        .clone();
    let parameters = input.parameters.as_ref();
    if parameters.is_some_and(|p| p.sample_count.is_some_and(|v| v != 1)) {
        return Err(TransformError::unsupported(
            "sampleCount",
            "native Sora requires sample fanout",
        ));
    }
    if parameters.is_some_and(|p| {
        p.person_generation.is_some() || p.negative_prompt.is_some() || p.enhance_prompt.is_some()
    }) {
        return Err(TransformError::unsupported(
            "parameters",
            "native Sora has no personGeneration/negativePrompt/enhancePrompt controls",
        ));
    }
    let seconds = match parameters.and_then(|p| p.duration_seconds) {
        Some(4) => o::NativeVideoSeconds::Four,
        Some(8) => o::NativeVideoSeconds::Eight,
        Some(12) => o::NativeVideoSeconds::Twelve,
        Some(_) => {
            return Err(TransformError::unsupported(
                "durationSeconds",
                "native Sora requires exactly 4, 8 or 12 seconds",
            ));
        }
        None => defaults
            .as_ref()
            .map(|v| v.seconds)
            .ok_or_else(|| TransformError::missing_metadata("veo.effective_duration"))?,
    };
    let aspect = parameters.and_then(|p| p.aspect_ratio.as_deref());
    let resolution = parameters.and_then(|p| p.resolution.as_deref());
    if aspect.is_some_and(|v| !matches!(v, "16:9" | "9:16"))
        || resolution.is_some_and(|v| v != "720p")
    {
        return Err(TransformError::unsupported(
            "veo.dimensions",
            "no exact native dimension mapping",
        ));
    }
    let size = match (aspect, resolution) {
        (Some("16:9"), Some("720p")) => o::NativeVideoSize::Landscape720,
        (Some("9:16"), Some("720p")) => o::NativeVideoSize::Portrait720,
        (None, None) => defaults
            .as_ref()
            .map(|v| v.size)
            .ok_or_else(|| TransformError::missing_metadata("veo.effective_dimensions"))?,
        (a, r) => {
            let default = defaults
                .as_ref()
                .ok_or_else(|| TransformError::missing_metadata("veo.effective_dimensions"))?;
            let da = match default.size {
                o::NativeVideoSize::Landscape720 => "16:9",
                o::NativeVideoSize::Portrait720 => "9:16",
                _ => {
                    return Err(TransformError::unsupported(
                        "veo.dimensions",
                        "no exact native dimension mapping",
                    ));
                }
            };
            if a.is_some_and(|v| v != da) || r.is_some_and(|v| v != "720p") {
                return Err(TransformError::unsupported(
                    "veo.dimensions",
                    "no exact native dimension mapping",
                ));
            }
            default.size
        }
    };
    // Even an omitted Veo dimension must be resolved to an actual Veo setting;
    // non-Veo native defaults cannot create an equivalent mapping.
    if matches!(
        size,
        o::NativeVideoSize::Portrait1024 | o::NativeVideoSize::Landscape1024
    ) {
        return Err(TransformError::unsupported(
            "veo.dimensions",
            "Veo effective dimensions cannot be native-only 1024 sizes",
        ));
    }
    let reference = instance
        .image
        .as_ref()
        .map(|image| {
            super::resources::published_url(image, resources).map(|url| {
                o::NativeInputReference::Url(o::NativeUrlReference::builder(url).build())
            })
        })
        .transpose()?;
    let mut body = o::NativeCreateVideoRequestBody::builder(prompt)
        .model(target_model)
        .seconds(seconds)
        .size(size)
        .build();
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
