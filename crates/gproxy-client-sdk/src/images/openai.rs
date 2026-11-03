use super::*;
use crate::media_support::{self as shared, assets, compatibility, count, metadata, warn};
use base64::Engine;
use protocol::openai::images as o;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenaiImageOptions {
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub quality: Option<o::ImageQuality>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub background: Option<o::ImageBackground>,
    pub output_compression: Option<i64>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub moderation: Option<o::ImageModeration>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub input_fidelity: Option<o::InputFidelity>,
    pub partial_images: Option<u8>,
    pub user: Option<String>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
pub enum OpenaiImageRequest {
    Generate(o::CreateImageRequestBody),
    Edit(o::EditImageJsonBody),
}
impl OpenaiImageRequest {
    pub(super) fn wire(&self) -> Result<(protocol::Operation, WireRequest<HttpBody>)> {
        use protocol::Operation as O;
        match self {
            Self::Generate(body) => Ok((
                O::CreateImage,
                crate::client::wire_json(
                    http::Method::POST,
                    "/v1/images/generations".into(),
                    body,
                )?,
            )),
            Self::Edit(body) => Ok((
                O::EditImage,
                crate::client::wire_json(http::Method::POST, "/v1/images/edits".into(), body)?,
            )),
        }
    }
}
pub fn render(request: &ImageRequest, streaming: bool) -> Result<Rendered<OpenaiImageRequest>> {
    request.check_inputs()?;
    let mut warnings = Vec::new();
    let options = &request.provider_options.openai;
    if request.seed.is_some() {
        warn(
            &mut warnings,
            "seed",
            "OpenAI image requests have no seed parameter",
        );
    }
    if request.provider_options.gemini != gemini::GeminiImageOptions::default() {
        compatibility(
            &mut warnings,
            "provider_options.gemini",
            "Gemini image options are omitted by OpenAI",
        );
    }
    let size = dimensions::openai(request, &mut warnings);
    let output = output_format(request.output_format.as_ref(), &mut warnings);
    let partial = options
        .partial_images
        .or(if streaming { Some(1) } else { None })
        .map(|v| Some(i64::from(v)));
    let native = if request.images.is_empty() {
        if options.input_fidelity.is_some() {
            warn(
                &mut warnings,
                "provider_options.openai.input_fidelity",
                "Input fidelity applies to image edits only",
            );
        }
        let mut body = o::CreateImageRequestBody::builder(request.prompt.clone()).build();
        body.model = Some(Some(request.model.id.clone()));
        body.n = Some(Some(i64::from(request.n.get())));
        body.size = size.map(Some);
        body.output_format = Some(Some(output));
        body.stream = Some(Some(streaming));
        body.partial_images = partial;
        body.quality = options.quality.map(Some);
        body.background = options.background.map(Some);
        body.moderation = options.moderation.map(Some);
        body.output_compression = options.output_compression.map(Some);
        body.user = options.user.clone();
        body.rest = options.rest.clone();
        OpenaiImageRequest::Generate(body)
    } else {
        let images = request
            .images
            .iter()
            .map(|image| reference(image, &mut warnings))
            .collect::<Result<_>>()?;
        let mut body = o::EditImageJsonBody::builder(images, request.prompt.clone()).build();
        body.model = Some(Some(request.model.id.clone()));
        body.n = Some(Some(i64::from(request.n.get())));
        body.size = size.map(|s| Some(o::EditedImageSize::Custom(s)));
        body.output_format = Some(Some(output));
        body.stream = Some(Some(streaming));
        body.partial_images = partial;
        body.background = options.background.map(Some);
        body.moderation = options.moderation.map(Some);
        body.output_compression = options.output_compression.map(Some);
        body.input_fidelity = options.input_fidelity.map(Some);
        body.user = options.user.clone();
        body.rest = options.rest.clone();
        body.quality = options.quality.and_then(|q| {
            use o::{EditedImageQuality as E, ImageQuality as Q};
            Some(Some(match q {
                Q::Low => E::Low,
                Q::Medium => E::Medium,
                Q::High => E::High,
                Q::Auto => E::Auto,
                Q::Xhigh => E::Xhigh,
                Q::Max => E::Max,
                _ => {
                    warn(
                        &mut warnings,
                        "provider_options.openai.quality",
                        "This quality has no edit equivalent and is omitted",
                    );
                    return None;
                }
            }))
        });
        body.mask = request
            .mask
            .as_ref()
            .map(|mask| {
                if mask
                    .content
                    .media_type
                    .as_deref()
                    .is_some_and(|m| m != "image/png")
                {
                    return Err(Error::invalid(
                        "OpenAI masks must be PNG; no transcoding is performed",
                    ));
                }
                reference(mask, &mut warnings)
            })
            .transpose()?;
        OpenaiImageRequest::Edit(body)
    };
    Ok(Rendered {
        request: native,
        warnings,
    })
}
fn reference(
    image: &ContentBlock<Image>,
    warnings: &mut Vec<Warning>,
) -> Result<o::ImageReference> {
    let url = assets::data_url(&image.content, AssetDialect::OpenaiImages)?;
    // upstream_docs/openai/docs/Create image edit.md:25: image_url maxLength 20971520.
    if url.chars().count() > 20_971_520 {
        return Err(Error::invalid(
            "OpenAI image reference exceeds the documented URI length limit",
        ));
    }
    if image.content.detail != Detail::Auto {
        warn(
            warnings,
            "images.detail",
            "OpenAI image editing has no input detail control",
        );
    }
    if image.cache.is_some() {
        warn(
            warnings,
            "images.cache",
            "Image operations have no cache breakpoint",
        );
    }
    if !image.provider_metadata.is_empty() {
        compatibility(
            warnings,
            "images.provider_metadata",
            "Image editing sends the image source, without conversation/output metadata",
        );
    }
    Ok(o::ImageReference::builder().image_url(url).build())
}
pub(super) fn output_format(
    value: Option<&ImageOutputFormat>,
    warnings: &mut Vec<Warning>,
) -> o::ImageOutputFormat {
    match value {
        Some(ImageOutputFormat::Jpeg) => o::ImageOutputFormat::Jpeg,
        Some(ImageOutputFormat::Webp) => o::ImageOutputFormat::Webp,
        Some(ImageOutputFormat::Unknown(_)) => {
            warn(
                warnings,
                "output_format",
                "Unknown output format uses OpenAI's PNG default",
            );
            o::ImageOutputFormat::Png
        }
        _ => o::ImageOutputFormat::Png,
    }
}
pub(super) fn mime(format: o::ImageOutputFormat) -> &'static str {
    match format {
        o::ImageOutputFormat::Png => "image/png",
        o::ImageOutputFormat::Jpeg => "image/jpeg",
        o::ImageOutputFormat::Webp => "image/webp",
    }
}
pub fn parse(
    mut response: o::ImagesResponse,
    effective_format: o::ImageOutputFormat,
) -> Result<ImageResult> {
    let format = response.output_format.unwrap_or(effective_format);
    let usage = usage(response.usage.as_ref())?;
    let mut images = Vec::new();
    for mut image in response.data.take().unwrap_or_default() {
        let source = if let Some(data) = image.b64_json.take() {
            Asset::Inline {
                bytes: base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(Error::invalid)?,
            }
        } else if let Some(url) = image.url.take() {
            Asset::Url { url }
        } else {
            return Err(Error::invalid("generated image has neither bytes nor URL"));
        };
        images.push(ContentBlock {
            content: Image {
                source,
                media_type: Some(mime(format).into()),
                detail: Detail::Auto,
            },
            cache: None,
            provider_metadata: [(Vendor::Openai, metadata(image)?)].into(),
        });
    }
    #[derive(Serialize)]
    struct Extras {
        background: Option<o::OutputImageBackground>,
        quality: Option<o::OutputImageQuality>,
        size: Option<o::OutputImageSize>,
        #[serde(flatten)]
        rest: protocol::Rest,
    }
    let extras = metadata(Extras {
        background: response.background,
        quality: response.quality,
        size: response.size,
        rest: response.rest,
    })?;
    Ok(ImageResult {
        images,
        text: Vec::new(),
        finish_reason: shared::io::stop(),
        usage,
        warnings: Vec::new(),
        response: ResponseMetadata {
            timestamp: Some(response.created),
            ..Default::default()
        },
        provider_metadata: [(Vendor::Openai, extras)].into(),
    })
}
pub(super) fn usage(value: Option<&o::ImageUsage>) -> Result<OperationUsage> {
    let Some(u) = value else {
        return Ok(OperationUsage::default());
    };
    Ok(OperationUsage {
        tokens: Usage {
            input_tokens: Some(count(u.input_tokens)?),
            output_tokens: Some(count(u.output_tokens)?),
            ..Default::default()
        },
        total_tokens: Some(count(u.total_tokens)?),
        ..Default::default()
    })
}
