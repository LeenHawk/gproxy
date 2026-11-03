use super::*;
pub(super) fn input_image(
    block: &ContentBlock<impl Sized>,
    image: &Image,
) -> Result<i::ResponseInputImage> {
    media::check_file_vendor(&image.source, Vendor::Openai)?;
    let source = &image.source;
    let mut value = i::ResponseInputImage::builder(
        i::ResponseInputImageType::ResponseInputImage,
        i::ImageDetail::Auto,
    )
    .build();
    if let Some(ImageMetadata::Input(meta)) = read_native(block, Vendor::Openai)? {
        value.rest = meta.rest;
        if meta.file_id_null {
            value.file_id = Some(None);
        }
        if meta.image_url_null {
            value.image_url = Some(None);
        }
    }
    value.detail = media::responses_detail(&image.detail).unwrap_or(i::ImageDetail::Auto);
    match source {
        Asset::ProviderFile { id, .. } => value.file_id = Some(Some(id.clone())),
        _ => value.image_url = Some(Some(data_url(source, image.media_type.as_deref())?)),
    };
    value.prompt_cache_breakpoint = media::responses_cache(block)?;
    Ok(value)
}

pub(super) fn input_document(
    block: &ContentBlock<impl Sized>,
    document: &Document,
) -> Result<i::ResponseInputFile> {
    let (original, mime) = media::document_bytes(document)?;
    media::check_file_vendor(&original, Vendor::Openai)?;
    let source = original.as_ref();
    input_file(
        block,
        source,
        mime,
        document.title.as_deref(),
        Some(&document.detail),
    )
}

pub(super) fn input_file(
    block: &ContentBlock<impl Sized>,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
    detail: Option<&Detail>,
) -> Result<i::ResponseInputFile> {
    let mut value =
        i::ResponseInputFile::builder(i::ResponseInputFileType::ResponseInputFile).build();
    let meta = read_native::<FileMetadata>(block, Vendor::Openai)?;
    let raw = meta.as_ref().is_some_and(|m| m.raw_base64);
    let had_detail = meta.as_ref().is_some_and(|m| m.detail_present);
    if let Some(meta) = meta {
        value.rest = meta.rest;
        if meta.file_id_null {
            value.file_id = Some(None);
        }
    }
    value.filename = filename.map(str::to_owned);
    match source {
        Asset::ProviderFile { vendor, id } => {
            if *vendor != Vendor::Openai {
                return Err(Error::invalid("provider files cannot cross vendors"));
            }
            value.file_id = Some(Some(id.clone()));
        }
        Asset::Url { url } => value.file_url = Some(url.clone()),
        Asset::Inline { bytes } => {
            value.file_data = Some(if raw {
                base64::engine::general_purpose::STANDARD.encode(bytes)
            } else {
                data_url(source, mime)?
            });
            value.filename = filename.map(str::to_owned);
        }
        Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "prepare application assets before rendering",
            ));
        }
    }
    value.detail = match detail {
        Some(Detail::Low) => Some(i::FileDetail::Low),
        Some(Detail::High) => Some(i::FileDetail::High),
        Some(Detail::Auto) => Some(i::FileDetail::Auto),
        Some(_) => None,
        None => value.detail,
    };
    // Auto is the native default; omit an override on newly-authored documents.
    if !had_detail && detail == Some(&Detail::Auto) && value.detail == Some(i::FileDetail::Auto) {
        value.detail = None;
    }
    value.prompt_cache_breakpoint = media::responses_cache(block)?;
    Ok(value)
}

pub(crate) fn is_output_image(block: &Block) -> Result<bool> {
    Ok(matches!(
        read_native::<ImageMetadata>(block, Vendor::Openai)?,
        Some(ImageMetadata::Generated(_))
    ))
}

pub(super) fn generated_image(block: &Block, image: &Image) -> Result<Option<i::InputItem>> {
    let Some(ImageMetadata::Generated(meta)) = read_native(block, Vendor::Openai)? else {
        return Ok(None);
    };
    let Asset::Inline { bytes } = &image.source else {
        return Err(Error::invalid(
            "generated image history requires inline bytes",
        ));
    };
    let mut value = i::ImageGenerationCall::builder(
        i::ImageGenerationCallType::ImageGenerationCall,
        meta.id,
        Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
        meta.status,
    )
    .rest(meta.rest)
    .build();
    value.error = meta.error;
    value.agent = meta.agent;
    value.action = meta.action;
    value.background = meta.background;
    value.output_format = match image.media_type.as_deref() {
        Some("image/png") => Some(Some(t::ImageOutputFormat::Png)),
        Some("image/jpeg") => Some(Some(t::ImageOutputFormat::Jpeg)),
        Some("image/webp") => Some(Some(t::ImageOutputFormat::Webp)),
        Some(_) => {
            return Err(Error::invalid(
                "native generated-image history requires PNG, JPEG or WebP",
            ));
        }
        None => meta.format_present.then_some(None),
    };
    value.quality = meta.quality;
    value.revised_prompt = meta.revised_prompt;
    value.size = meta.size;
    Ok(Some(i::InputItem::ImageGenerationCall(value)))
}

pub(super) fn function_output(block: &ToolResultBlock) -> Result<i::FunctionOutputContent> {
    Ok(match &block.content {
        ToolResultContent::Unknown(_) => {
            return Err(Error::invalid("unknown persisted tool content"));
        }
        ToolResultContent::Text(text) => {
            let mut value = i::FunctionOutputText::builder(
                i::ResponseInputTextType::ResponseInputText,
                text.text.clone(),
            )
            .build();
            if let Some(native) = read_native::<Remainder>(block, Vendor::Openai)? {
                value.rest = native.rest;
            }
            value.prompt_cache_breakpoint = media::responses_cache(block)?.map(Some);
            i::FunctionOutputContent::Text(value)
        }
        ToolResultContent::Image(image) => {
            let input = input_image(block, image)?;
            let mut value =
                i::FunctionOutputImage::builder(i::ResponseInputImageType::ResponseInputImage)
                    .build();
            value.file_id = input.file_id;
            value.image_url = input.image_url;
            value.detail = Some(Some(input.detail));
            value.prompt_cache_breakpoint = input.prompt_cache_breakpoint.map(Some);
            value.rest = input.rest;
            i::FunctionOutputContent::Image(value)
        }
        ToolResultContent::Document(document) => {
            let input = input_document(block, document)?;
            let mut value =
                i::FunctionOutputFile::builder(i::ResponseInputFileType::ResponseInputFile).build();
            value.file_id = input.file_id;
            value.file_data = input.file_data.map(Some);
            value.file_url = input.file_url.map(Some);
            value.filename = input.filename.map(Some);
            value.detail = input.detail;
            value.prompt_cache_breakpoint = input.prompt_cache_breakpoint.map(Some);
            value.rest = input.rest;
            i::FunctionOutputContent::File(value)
        }
    })
}
