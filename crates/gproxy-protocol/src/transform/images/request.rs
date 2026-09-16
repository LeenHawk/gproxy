use crate::{
    WireRequest,
    capability::ResourceReference,
    transform::{Converted, Report, TransformError},
    wire::{
        gemini as g,
        openai::{images as o, responses as r},
    },
};
use r::{input as ri, tools as rt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageDialect {
    Responses,
    Gemini,
}
#[derive(Debug, Clone, PartialEq)]
pub enum ImageInput {
    Create(o::CreateImageRequestBody),
    Edit(o::EditImageJsonBody),
}
/// The generation model is selected by the host. An image tool model is a
/// separate routing decision; an absent override retains the source image model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageTargetModels {
    pub generation_model: String,
    pub image_tool_model: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImageInput {
    pub reference: ResourceReference,
    pub bytes_base64: String,
    pub mime_type: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRequestContext {
    pub source_model: Option<String>,
    pub target_models: ImageTargetModels,
    pub n: usize,
    pub response_format: Option<o::ImageResponseFormat>,
    pub output_format: Option<o::ImageOutputFormat>,
    pub requested_size: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResponseFacts {
    pub created: i64,
    /// Host clock snapshot; the protocol never reads a native clock.
    pub observed_at: std::time::SystemTime,
    /// Host-resolved source endpoint/model default when response_format is absent.
    pub default_response_format: o::ImageResponseFormat,
    pub publish_expires_at: Option<std::time::SystemTime>,
    pub operation_id: String,
}
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ImageDialectBody {
    Responses(WireRequest<r::GenerateContentRequestBody>),
    Gemini(WireRequest<g::GenerateContentRequestBody>),
}
pub struct PreparedImageRequest {
    pub body: ImageDialectBody,
    pub context: ImageRequestContext,
}

struct Options {
    prompt: String,
    model: Option<String>,
    n: i64,
    response_format: Option<o::ImageResponseFormat>,
    output_format: Option<o::ImageOutputFormat>,
    size: Option<String>,
    tool: rt::ImageGenerationTool,
    images: Vec<ResourceReference>,
    mask: Option<ResourceReference>,
    user: Option<String>,
}
fn unsupported(field: &str) -> TransformError {
    TransformError::unsupported(
        field,
        "no exact equivalent in selected image generation API",
    )
}
fn options(input: &ImageInput) -> Result<Options, TransformError> {
    let (
        prompt,
        model,
        n,
        format,
        output,
        size,
        background,
        moderation,
        compression,
        _partial,
        _stream,
        quality,
        fidelity,
        images,
        mask,
        user,
    ) = match input {
        ImageInput::Create(v) => {
            let quality = v
                .quality
                .flatten()
                .map(|q| match q {
                    o::ImageQuality::Low => Ok(rt::ImageQuality::Low),
                    o::ImageQuality::Medium => Ok(rt::ImageQuality::Medium),
                    o::ImageQuality::High => Ok(rt::ImageQuality::High),
                    o::ImageQuality::Auto => Ok(rt::ImageQuality::Auto),
                    _ => Err(unsupported("quality")),
                })
                .map(crate::transform::optional)
                .transpose()?
                .flatten();
            (
                v.prompt.clone(),
                v.model.clone().flatten(),
                v.n.flatten().unwrap_or(1),
                v.response_format.flatten(),
                v.output_format.flatten(),
                v.size.clone().flatten(),
                v.background.flatten(),
                v.moderation.flatten(),
                v.output_compression.flatten(),
                v.partial_images.flatten(),
                v.stream.flatten(),
                quality,
                None,
                Vec::new(),
                None,
                v.user.clone(),
            )
        }
        ImageInput::Edit(v) => {
            let quality = v.quality.flatten().map(|q| match q {
                o::EditedImageQuality::Low => rt::ImageQuality::Low,
                o::EditedImageQuality::Medium => rt::ImageQuality::Medium,
                o::EditedImageQuality::High => rt::ImageQuality::High,
                o::EditedImageQuality::Auto => rt::ImageQuality::Auto,
            });
            let size = v.size.flatten().map(|s| {
                match s {
                    o::EditedImageSize::Auto => "auto",
                    o::EditedImageSize::Square => "1024x1024",
                    o::EditedImageSize::Landscape => "1536x1024",
                    o::EditedImageSize::Portrait => "1024x1536",
                }
                .to_owned()
            });
            (
                v.prompt.clone(),
                v.model.clone().flatten(),
                v.n.flatten().unwrap_or(1),
                None,
                v.output_format.flatten(),
                size,
                v.background.flatten(),
                v.moderation.flatten(),
                v.output_compression.flatten(),
                v.partial_images.flatten(),
                v.stream.flatten(),
                quality,
                v.input_fidelity.flatten(),
                v.images
                    .iter()
                    .map(image_reference)
                    .filter_map(|value| crate::transform::optional(value).transpose())
                    .collect::<Result<Vec<_>, _>>()?,
                v.mask
                    .as_ref()
                    .map(image_reference)
                    .map(crate::transform::optional)
                    .transpose()?
                    .flatten(),
                v.user.clone(),
            )
        }
    };

    let mut tool = rt::ImageGenerationTool::builder().build();
    tool.action = Some(if images.is_empty() {
        rt::ImageAction::Generate
    } else {
        rt::ImageAction::Edit
    });
    tool.background = background.map(|v| match v {
        o::ImageBackground::Auto => rt::ImageBackground::Auto,
        o::ImageBackground::Opaque => rt::ImageBackground::Opaque,
        o::ImageBackground::Transparent => rt::ImageBackground::Transparent,
    });
    tool.moderation = moderation.map(|v| match v {
        o::ImageModeration::Auto => rt::Moderation::Auto,
        o::ImageModeration::Low => rt::Moderation::Low,
    });
    tool.quality = quality;
    tool.input_fidelity = fidelity.map(|v| {
        Some(match v {
            o::InputFidelity::Low => rt::InputFidelity::Low,
            o::InputFidelity::High => rt::InputFidelity::High,
        })
    });
    tool.output_format = output.map(|v| match v {
        o::ImageOutputFormat::Png => rt::ImageOutputFormat::Png,
        o::ImageOutputFormat::Jpeg => rt::ImageOutputFormat::Jpeg,
        o::ImageOutputFormat::Webp => rt::ImageOutputFormat::Webp,
    });
    tool.output_compression = compression.map(Into::into);
    tool.size = size.clone();
    Ok(Options {
        prompt,
        model,
        n,
        response_format: format,
        output_format: output,
        size,
        tool,
        images,
        mask,
        user,
    })
}
pub fn image_reference(v: &o::ImageReference) -> Result<ResourceReference, TransformError> {
    match (&v.file_id, &v.image_url) {
        (Some(id), None) if !id.trim().is_empty() => Ok(ResourceReference::Id(id.clone())),
        (None, Some(url)) if !url.trim().is_empty() => Ok(ResourceReference::Url(url.clone())),
        _ => Err(TransformError::shape(
            "image.reference",
            "exactly one nonempty file_id or image_url required",
        )),
    }
}
/// Selects the source resources and return parameters used by the adapter.
pub fn preflight(
    input: &ImageInput,
    dialect: ImageDialect,
    models: &ImageTargetModels,
) -> Result<(ImageRequestContext, Vec<ResourceReference>, Report), TransformError> {
    let v = options(input)?;

    let mut report = Report::default();
    if dialect == ImageDialect::Gemini {
        // Gemini imageSize is a resolution class, not an exact pixel size.

        if v.user.is_some() {
            report.omitted("user", "Gemini has no equivalent user attribution field");
        }
        if v.model.is_some() {
            report.changed("model", "host selected the Gemini image generation model");
        }
    }
    let mut refs = v.images;
    if let Some(mask) = v.mask.filter(|_| dialect == ImageDialect::Responses) {
        refs.push(mask);
    }
    let n = usize::try_from(v.n).unwrap_or(1);
    if n > 1 {
        report.changed("n", "one native image generation call per requested image");
    }
    Ok((
        ImageRequestContext {
            source_model: v.model,
            target_models: models.clone(),
            n,
            response_format: v.response_format,
            output_format: v.output_format,
            requested_size: v.size,
        },
        refs,
        report,
    ))
}
fn resolved<'a>(
    reference: &ResourceReference,
    values: &'a [ResolvedImageInput],
) -> Result<&'a ResolvedImageInput, TransformError> {
    let mut matches = values.iter().filter(|v| &v.reference == reference);
    let v = matches
        .next()
        .ok_or_else(|| TransformError::missing_metadata("image.resource"))?;

    Ok(v)
}
fn data_url(v: &ResolvedImageInput) -> String {
    format!("data:{};base64,{}", v.mime_type, v.bytes_base64)
}
/// Builds ONE call; `context.n` is the adapter's total call count.
pub fn build_request(
    input: ImageInput,
    dialect: ImageDialect,
    models: &ImageTargetModels,
    values: &[ResolvedImageInput],
) -> Result<Converted<PreparedImageRequest>, TransformError> {
    let (context, _, report) = preflight(&input, dialect, models)?;
    let mut v = options(&input)?;
    let body = match dialect {
        ImageDialect::Responses => {
            v.tool.model = models.image_tool_model.clone().or(v.model);
            if let Some(mask) = v.mask {
                let mask = resolved(&mask, values)?;
                v.tool.input_image_mask = Some(
                    rt::InputImageMask::builder()
                        .image_url(data_url(mask))
                        .build(),
                );
            }
            let mut content = vec![ri::InputContent::Text(
                ri::ResponseInputText::builder(
                    ri::ResponseInputTextType::ResponseInputText,
                    v.prompt,
                )
                .build(),
            )];
            for reference in &v.images {
                content.push(ri::InputContent::Image(
                    ri::ResponseInputImage::builder(
                        ri::ResponseInputImageType::ResponseInputImage,
                        ri::ImageDetail::Auto,
                    )
                    .image_url(Some(data_url(resolved(reference, values)?)))
                    .build(),
                ));
            }
            let mut body = r::GenerateContentRequestBody::builder()
                .model(models.generation_model.clone())
                .input(ri::Input::Items(vec![ri::InputItem::Message(
                    ri::InputMessage::builder(content, ri::InputMessageRole::User).build(),
                )]))
                .tools(vec![rt::Tool::ImageGeneration(v.tool)])
                .tool_choice(ri::ToolChoice::Hosted(
                    ri::ToolChoiceHosted::builder(ri::ToolChoiceHostedType::ImageGeneration)
                        .build(),
                ))
                .build();
            body.user = v.user;
            body.parallel_tool_calls = Some(Some(false));
            body.max_tool_calls = Some(Some(1));
            body.stream = Some(Some(false));
            ImageDialectBody::Responses(request("/v1/responses".into(), body))
        }
        ImageDialect::Gemini => {
            let mut parts = vec![g::Part::builder().text(v.prompt).build()];
            for reference in &v.images {
                let image = resolved(reference, values)?;
                parts.push(
                    g::Part::builder()
                        .inline_data(
                            g::Blob::builder(image.mime_type.clone(), image.bytes_base64.clone())
                                .build(),
                        )
                        .build(),
                );
            }
            let config = g::GenerationConfig::builder()
                .response_modalities(vec![g::Modality::Image])
                .build();
            let body = g::GenerateContentRequestBody::builder(vec![
                g::Content::builder().role("user").parts(parts).build(),
            ])
            .generation_config(config)
            .build();
            ImageDialectBody::Gemini(request(
                format!("/v1beta/models/{}:generateContent", models.generation_model),
                body,
            ))
        }
    };
    Ok(Converted {
        value: PreparedImageRequest { body, context },
        report,
    })
}
fn request<T>(path: String, body: T) -> WireRequest<T> {
    WireRequest {
        method: http::Method::POST,
        path,
        query: None,
        headers: http::HeaderMap::new(),
        body,
    }
}
