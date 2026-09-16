use super::{
    dimensions::dimensions,
    resources::{frame_from_resource, frame_role, image_resource, published_url, video_url},
};
use crate::wire::DeclaredFields;
use std::collections::BTreeMap;

use crate::{
    Rest,
    transform::{Converted, Report, TransformError},
    wire::{gemini::video as g, openai::video as o},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoResourceRole {
    FirstFrame,
    LastFrame,
    ReferenceImage,
    ReferenceVideo,
    ReferenceAudio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoResourceNeed {
    pub reference: String,
    pub role: VideoResourceRole,
}

/// Facts supplied by a host after URL resolution/download or publication.
/// `bytes_base64_encoded`/`mime_type` are needed by Gemini; `url` is needed by
/// the OpenRouter-shaped request.  No URL or bytes are inferred by a mapper.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedVideoResource {
    pub reference: String,
    pub url: Option<String>,
    pub bytes_base64_encoded: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VeoRequestContext {
    pub source_model: Option<String>,
    pub target_model: String,
    pub source_request: Option<o::CreateVideoRequestBody>,
    pub source_veo_request: Option<g::PredictLongRunningRequestBody>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedVeoRequest {
    pub body: g::PredictLongRunningRequestBody,
    pub context: VeoRequestContext,
}

pub fn resource_needs(input: &o::CreateVideoRequestBody) -> Vec<VideoResourceNeed> {
    let mut needs = Vec::new();
    if let Some(frames) = &input.frame_images {
        for frame in frames {
            needs.push(VideoResourceNeed {
                reference: frame.image_url.url.clone(),
                role: match frame.frame_type {
                    o::FrameType::FirstFrame => VideoResourceRole::FirstFrame,
                    o::FrameType::LastFrame => VideoResourceRole::LastFrame,
                },
            });
        }
    }
    if let Some(references) = &input.input_references {
        for reference in references {
            match reference {
                o::InputReference::Image(value) => needs.push(VideoResourceNeed {
                    reference: value.image_url.url.clone(),
                    role: VideoResourceRole::ReferenceImage,
                }),
                o::InputReference::Video(value) => needs.push(VideoResourceNeed {
                    reference: value.video_url.url.clone(),
                    role: VideoResourceRole::ReferenceVideo,
                }),
                o::InputReference::Audio(value) => needs.push(VideoResourceNeed {
                    reference: value.audio_url.url.clone(),
                    role: VideoResourceRole::ReferenceAudio,
                }),
            }
        }
    }
    needs
}

pub fn openai_to_gemini_request(
    input: o::CreateVideoRequestBody,
    target_model: &str,
    resources: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<Converted<PreparedVeoRequest>, TransformError> {
    let input = input.into_declared();
    let target_model = non_empty(target_model, "target_model")?;
    let source_model = Some(non_empty(&input.model, "source_model")?);
    reject_openai_only(&input)?;
    let (aspect_ratio, resolution) = dimensions(&input)?;

    let mut instance = g::VideoGenerationInstance {
        prompt: input.prompt.clone(),
        image: None,
        video: None,
        last_frame: None,
        reference_images: None,
        rest: Rest::new(),
    };
    if let Some(frames) = &input.frame_images {
        for frame in frames {
            let resource = image_resource(&frame.image_url.url, resources, frame_role(frame))?;
            match frame.frame_type {
                o::FrameType::FirstFrame => {
                    if instance.image.is_some() {
                        return Err(TransformError::new(
                            crate::transform::TransformErrorKind::Conflict,
                            "frame_images",
                            "multiple first frames are ambiguous",
                        ));
                    }
                    instance.image = Some(resource);
                }
                o::FrameType::LastFrame => {
                    if instance.last_frame.is_some() {
                        return Err(TransformError::new(
                            crate::transform::TransformErrorKind::Conflict,
                            "frame_images",
                            "multiple last frames are ambiguous",
                        ));
                    }
                    instance.last_frame = Some(resource);
                }
            }
        }
    }
    if let Some(references) = &input.input_references {
        let mut output = Vec::with_capacity(references.len());
        for reference in references {
            match reference {
                o::InputReference::Image(value) => output.push(g::VideoReferenceImage {
                    image: Some(image_resource(
                        &value.image_url.url,
                        resources,
                        VideoResourceRole::ReferenceImage,
                    )?),
                    reference_type: Some(g::VideoReferenceType::Asset),
                    rest: Rest::new(),
                }),
                o::InputReference::Video(value) => {
                    let resource =
                        super::resources::bound(&value.video_url.url, resources, "video/")?;
                    let encoded = resource.bytes_base64_encoded.clone().ok_or_else(|| {
                        TransformError::missing_metadata(format!(
                            "video resource {} bytes",
                            value.video_url.url
                        ))
                    })?;
                    let mime = resource.mime_type.clone().ok_or_else(|| {
                        TransformError::missing_metadata(format!(
                            "video resource {} mime",
                            value.video_url.url
                        ))
                    })?;
                    instance.video = Some(g::Video {
                        uri: None,
                        encoded_video: Some(encoded),
                        encoding: Some(mime),
                        rest: Rest::new(),
                    });
                }
                o::InputReference::Audio(_) => {
                    return Err(TransformError::unsupported(
                        "input_references.audio_url",
                        "Developer API Veo has no audio reference input",
                    ));
                }
            }
            if let o::InputReference::Image(_) = reference {
                // Image references are appended above.  Video references use
                // the dedicated instance.video field and cannot be referenceImages.
                continue;
            }
        }
        if !output.is_empty() {
            instance.reference_images = Some(output);
        }
    }

    let mut parameters = g::VideoGenerationParameters {
        sample_count: None,
        duration_seconds: input
            .duration
            .map(|value| {
                i32::try_from(value)
                    .map_err(|_| TransformError::shape("duration", "overflows Gemini"))
            })
            .map(crate::transform::optional)
            .transpose()?
            .flatten(),
        aspect_ratio,
        resolution,
        person_generation: None,
        negative_prompt: None,
        enhance_prompt: None,
        rest: Rest::new(),
    };
    super::options::to_gemini(input.provider.as_ref(), &mut parameters)?;

    let body = g::PredictLongRunningRequestBody {
        instances: vec![instance],
        parameters: Some(parameters),
        webhook_config: None,
        rest: Rest::new(),
    };
    Ok(Converted {
        value: PreparedVeoRequest {
            body,
            context: VeoRequestContext {
                source_model,
                target_model,
                source_request: Some(input),
                source_veo_request: None,
            },
        },
        report: Report::default(),
    })
}

pub fn gemini_to_openai_request(
    input: g::PredictLongRunningRequestBody,
    target_model: &str,
    resources: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<Converted<PreparedOpenAiRequest>, TransformError> {
    let input = input.into_declared();
    let target_model = non_empty(target_model, "target_model")?;
    let source_veo_request = input.clone();

    let instance = input.instances.into_iter().next().expect("length checked");
    let mut frame_images = Vec::new();
    if let Some(image) = instance.image {
        frame_images.push(frame_from_resource(
            &image,
            o::FrameType::FirstFrame,
            resources,
        )?);
    }
    if let Some(image) = instance.last_frame {
        frame_images.push(frame_from_resource(
            &image,
            o::FrameType::LastFrame,
            resources,
        )?);
    }
    let mut input_references = Vec::new();
    if let Some(video) = instance.video {
        input_references.push(o::InputReference::Video(o::InputVideoReference {
            video_url: o::ReferenceUrl {
                url: video_url(&video, resources)?,
                rest: Rest::new(),
            },
            rest: Rest::new(),
        }));
    }
    for reference in instance.reference_images.unwrap_or_default() {
        let image = reference
            .image
            .ok_or_else(|| TransformError::missing_metadata("referenceImages[].image"))?;
        let url = published_url(&image, resources)?;
        input_references.push(o::InputReference::Image(o::InputImageReference {
            image_url: o::ReferenceUrl {
                url,
                rest: Rest::new(),
            },
            rest: Rest::new(),
        }));
    }
    let parameters = input.parameters.unwrap_or(g::VideoGenerationParameters {
        sample_count: None,
        duration_seconds: None,
        aspect_ratio: None,
        resolution: None,
        person_generation: None,
        negative_prompt: None,
        enhance_prompt: None,
        rest: Rest::new(),
    });

    let provider = super::options::to_openrouter(&parameters);

    let body = o::CreateVideoRequestBody {
        model: target_model.clone(),
        prompt: instance.prompt,
        duration: parameters.duration_seconds.map(i64::from),
        aspect_ratio: parse_enum(parameters.aspect_ratio.as_deref(), "aspect_ratio")?,
        resolution: parse_enum(parameters.resolution.as_deref(), "resolution")?,
        size: None,
        seed: None,
        frame_images: if frame_images.is_empty() {
            None
        } else {
            Some(frame_images)
        },
        input_references: if input_references.is_empty() {
            None
        } else {
            Some(input_references)
        },
        generate_audio: None,
        callback_url: None,
        provider,
        rest: Rest::new(),
    };
    Ok(Converted {
        value: PreparedOpenAiRequest {
            body,
            context: VeoRequestContext {
                source_model: None,
                target_model,
                source_request: None,
                source_veo_request: Some(source_veo_request),
            },
        },
        report: Report::default(),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedOpenAiRequest {
    pub body: o::CreateVideoRequestBody,
    pub context: VeoRequestContext,
}

fn reject_openai_only(_input: &o::CreateVideoRequestBody) -> Result<(), TransformError> {
    Ok(())
}

fn non_empty(value: &str, field: &str) -> Result<String, TransformError> {
    if value.trim().is_empty() {
        Err(TransformError::missing_metadata(field))
    } else {
        Ok(value.to_owned())
    }
}

pub(super) fn enum_string<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .expect("wire enum serializes")
        .as_str()
        .expect("wire enum is a string")
        .to_owned()
}

fn parse_enum<T: serde::de::DeserializeOwned>(
    value: Option<&str>,
    field: &str,
) -> Result<Option<T>, TransformError> {
    value
        .map(|value| {
            serde_json::from_value(Value::String(value.to_owned())).map_err(|_| {
                TransformError::unsupported(field, "value has no OpenRouter equivalent")
            })
        })
        .transpose()
}

use serde_json::Value;
