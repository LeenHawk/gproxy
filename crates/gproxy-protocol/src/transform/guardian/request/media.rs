use super::*;
use base64::Engine as _;

#[derive(Clone)]
pub(crate) enum Media {
    Image {
        url: String,
        detail: Option<source::ImageDetail>,
    },
    Audio {
        url: String,
    },
}

pub(super) fn media(input: &source::GuardianRequestBody) -> Result<Vec<Media>, TransformError> {
    let mut output = Vec::new();
    for entry in &input.input {
        let add_output = |body: &source::FunctionCallOutputBody, output: &mut Vec<Media>| {
            if let source::FunctionCallOutputBody::ContentItems(items) = body {
                for item in items {
                    match item {
                        source::FunctionCallOutputContentItem::InputImage(v) => {
                            output.push(Media::Image {
                                url: v.image_url.clone(),
                                detail: v.detail.clone(),
                            })
                        }
                        source::FunctionCallOutputContentItem::InputAudio(v) => {
                            output.push(Media::Audio {
                                url: v.audio_url.clone(),
                            })
                        }
                        _ => {}
                    }
                }
            }
        };
        match entry {
            source::ClientResponseItem::Message(v) => {
                for item in &v.content {
                    match item {
                        source::ContentItem::InputImage(x) => output.push(Media::Image {
                            url: x.image_url.clone(),
                            detail: x.detail.clone(),
                        }),
                        source::ContentItem::InputAudio(x) => output.push(Media::Audio {
                            url: x.audio_url.clone(),
                        }),
                        _ => {}
                    }
                }
            }
            source::ClientResponseItem::FunctionCallOutput(v) => add_output(&v.output, &mut output),
            source::ClientResponseItem::CustomToolCallOutput(v) => {
                add_output(&v.output, &mut output)
            }
            source::ClientResponseItem::ImageGenerationCall(v) => {
                let bytes = STANDARD.decode(&v.result).map_err(|e| {
                    TransformError::shape("guardian.image_generation.result", e.to_string())
                })?;
                let info = crate::transform::images::inspect_image(&bytes).map_err(|e| {
                    TransformError::shape("guardian.image_generation.result", e.detail())
                })?;
                output.push(Media::Image {
                    url: format!("data:{};base64,{}", info.mime(), v.result),
                    detail: None,
                });
            }
            _ => {}
        }
    }
    Ok(output)
}

pub(super) fn data_uri(value: &str) -> Result<(String, String), TransformError> {
    let (mime, data) = value
        .strip_prefix("data:")
        .and_then(|v| v.split_once(";base64,"))
        .ok_or_else(|| {
            TransformError::missing_metadata(
                "guardian media requires a resolved data URI with MIME",
            )
        })?;
    STANDARD
        .decode(data)
        .map_err(|e| TransformError::shape("guardian.media", e.to_string()))?;
    Ok((mime.into(), data.into()))
}

pub(super) fn detail(value: &Option<source::ImageDetail>) -> r::ImageDetail {
    match value {
        None | Some(source::ImageDetail::Auto) => r::ImageDetail::Auto,
        Some(source::ImageDetail::Low) => r::ImageDetail::Low,
        Some(source::ImageDetail::High) => r::ImageDetail::High,
        Some(source::ImageDetail::Original) => r::ImageDetail::Original,
    }
}

impl Media {
    pub(crate) fn url(&self) -> &str {
        match self {
            Self::Image { url, .. } | Self::Audio { url } => url,
        }
    }
    pub(crate) fn set_url(&mut self, value: String) {
        match self {
            Self::Image { url, .. } | Self::Audio { url } => *url = value,
        }
    }
    pub(crate) fn needs_resolution(&self, target: GuardianTarget) -> bool {
        !self.url().starts_with("data:")
            && (target == GuardianTarget::Gemini
                || matches!(
                    (target, self),
                    (GuardianTarget::OpenAiChat, Self::Audio { .. })
                ))
    }
}
