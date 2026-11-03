use super::*;
use crate::media_support::{self as shared, assets, compatibility, native_metadata, warn};
use crate::provider_metadata::{MediaMetadata, PartMetadata, part_fields};
use base64::Engine;
use protocol::gemini as g;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GeminiImageOptions {
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub generation_config: Option<g::GenerationConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub safety_settings: Option<Vec<g::SafetySetting>>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
pub fn render(request: &ImageRequest) -> Result<Rendered<g::GenerateContentRequestBody>> {
    request.check_inputs()?;
    let options = &request.provider_options.gemini;
    let mut warnings = Vec::new();
    if request.provider_options.openai != openai::OpenaiImageOptions::default() {
        compatibility(
            &mut warnings,
            "provider_options.openai",
            "OpenAI image options are omitted by Gemini",
        );
    }
    let mut parts = vec![g::Part::builder().text(request.prompt.clone()).build()];
    for image in &request.images {
        parts.push(assets::image_part(
            image,
            AssetDialect::GeminiImages,
            &mut warnings,
        )?);
    }
    let mut body = g::GenerateContentRequestBody::builder(vec![
        g::Content::builder().role("user").parts(parts).build(),
    ])
    .build();
    let mut config = options
        .generation_config
        .clone()
        .unwrap_or_else(|| g::GenerationConfig::builder().build());
    if config.response_modalities.is_none() {
        config.response_modalities = Some(vec![g::Modality::Text, g::Modality::Image]);
    }
    let (aspect, tier) = dimensions::gemini(request, &mut warnings);
    if aspect.is_some() || tier.is_some() {
        let image = config
            .image_config
            .get_or_insert_with(|| g::ImageConfig::builder().build());
        if aspect.is_some() {
            if image.aspect_ratio.is_some() {
                compatibility(
                    &mut warnings,
                    "aspect_ratio",
                    "Native imageConfig overrides the portable ratio",
                );
            } else {
                image.aspect_ratio = aspect;
            }
        }
        if tier.is_some() {
            if image.image_size.is_some() {
                compatibility(
                    &mut warnings,
                    "resolution",
                    "Native imageConfig overrides the portable tier",
                );
            } else {
                image.image_size = tier;
            }
        }
    }
    if let Some(seed) = request.seed {
        if config.seed.is_some() {
            compatibility(
                &mut warnings,
                "seed",
                "Native generationConfig seed overrides the portable seed",
            );
        } else {
            config.seed = Some(seed);
        }
    }
    if let Some(format) = &request.output_format {
        match format {
            ImageOutputFormat::Jpeg => {
                let image = config
                    .response_format
                    .get_or_insert_with(|| g::ResponseFormatConfig::builder().build())
                    .image
                    .get_or_insert_with(|| g::ImageResponseFormat::builder().build());
                if image.mime_type.is_some() {
                    compatibility(
                        &mut warnings,
                        "output_format",
                        "Native response_format overrides the portable format",
                    );
                } else {
                    image.mime_type = Some(g::ImageMimeType::ImageJpeg);
                }
            }
            _ => warn(
                &mut warnings,
                "output_format",
                "Gemini's documented response_format exposes JPEG only; requested format is omitted",
            ),
        }
    }
    body.generation_config = Some(config);
    body.safety_settings = options.safety_settings.clone();
    body.rest = options.rest.clone();
    Ok(Rendered {
        request: body,
        warnings,
    })
}
pub fn parse(mut response: g::GenerateContentResponseBody) -> Result<ImageResult> {
    let mut warnings = Vec::new();
    shared::gemini::candidate_warning(&response, &mut warnings);
    let info = shared::gemini::response(&response);
    let usage = shared::gemini::usage(response.usage_metadata.as_ref())?;
    let mut images = Vec::new();
    let mut text = Vec::new();
    let mut reason = None;
    if let Some(mut candidate) = response.candidates.take().into_iter().flatten().next() {
        reason = candidate.finish_reason;
        for mut part in candidate
            .content
            .take()
            .into_iter()
            .flat_map(|c| c.parts.into_iter().flatten())
        {
            let mut native = PartMetadata::default();
            if let Some(blob) = part.inline_data.take() {
                if !blob.mime_type.starts_with("image/") {
                    return Err(Error::invalid(
                        "image generation returned non-image binary data",
                    ));
                }
                let image = Image {
                    source: Asset::Inline {
                        bytes: base64::engine::general_purpose::STANDARD
                            .decode(blob.data)
                            .map_err(Error::invalid)?,
                    },
                    media_type: Some(blob.mime_type),
                    detail: Detail::Auto,
                };
                native.inline_data = Some(MediaMetadata {
                    display_name: blob.display_name,
                    rest: blob.rest,
                    ..Default::default()
                });
                native.fields = part_fields(part);
                images.push(ContentBlock {
                    content: image,
                    cache: None,
                    provider_metadata: [(Vendor::Gemini, native_metadata(native)?)].into(),
                });
            } else if let Some(file) = part.file_data.take() {
                let image = Image {
                    source: Asset::Url { url: file.file_uri },
                    media_type: file.mime_type,
                    detail: Detail::Auto,
                };
                native.file_data = Some(MediaMetadata {
                    display_name: file.display_name,
                    rest: file.rest,
                    ..Default::default()
                });
                native.fields = part_fields(part);
                images.push(ContentBlock {
                    content: image,
                    cache: None,
                    provider_metadata: [(Vendor::Gemini, native_metadata(native)?)].into(),
                });
            } else if let Some(value) = part.text.take() {
                let content = if part.thought == Some(true) {
                    Content::Reasoning(Reasoning { text: value })
                } else {
                    Content::Text(Text {
                        refusal: false,
                        text: value,
                        citations: Vec::new(),
                    })
                };
                native.fields = part_fields(part);
                text.push(Block {
                    content,
                    cache: None,
                    provider_metadata: [(Vendor::Gemini, native_metadata(native)?)].into(),
                });
            } else {
                compatibility(
                    &mut warnings,
                    "response.parts",
                    "An unrecognized image-response part is retained as Opaque",
                );
                text.push(Block {
                    content: Content::Opaque,
                    cache: None,
                    provider_metadata: [(Vendor::Gemini, native_metadata(part)?)].into(),
                });
            }
        }
    }
    Ok(ImageResult {
        images,
        text,
        finish_reason: shared::gemini::finish(reason, response.prompt_feedback.as_ref())?,
        usage,
        warnings,
        response: info,
        provider_metadata: [(Vendor::Gemini, shared::gemini::extras(&response)?)].into(),
    })
}
