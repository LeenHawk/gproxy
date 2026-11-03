use super::*;
use base64::Engine;
use protocol::gemini as g;
pub(crate) fn check(
    source: &Asset,
    mime: Option<&str>,
    target: AssetDialect,
    kind: MediaKind,
) -> Result<()> {
    if let Some(mime) = mime
        && ((kind == MediaKind::Image && !mime.starts_with("image/"))
            || (kind == MediaKind::Audio
                && !mime.starts_with("audio/")
                && !mime.starts_with("video/")))
    {
        return Err(Error::invalid(
            "media MIME type does not match its input kind",
        ));
    }
    crate::assets::resolve::check_asset(target, kind, AssetLocation::Message, source, mime, None)
}
pub(crate) fn part(
    source: &Asset,
    mime: Option<&str>,
    target: AssetDialect,
    kind: MediaKind,
) -> Result<g::Part> {
    check(source, mime, target, kind)?;
    let mut part = g::Part::builder().build();
    match source {
        Asset::Inline { bytes } => {
            part.inline_data = Some(
                g::Blob::builder(
                    mime.ok_or_else(|| Error::invalid("inline media requires a MIME type"))?
                        .into(),
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                )
                .build(),
            )
        }
        Asset::Url { url } => {
            let mut file = g::FileData::builder(url.clone()).build();
            file.mime_type = mime.map(str::to_owned);
            part.file_data = Some(file);
        }
        _ => {
            return Err(Error::invalid(
                "resolve media to inline bytes or a URL before rendering",
            ));
        }
    }
    Ok(part)
}
#[cfg(feature = "images")]
pub(crate) fn data_url(image: &Image, target: AssetDialect) -> Result<String> {
    check(
        &image.source,
        image.media_type.as_deref(),
        target,
        MediaKind::Image,
    )?;
    match &image.source {
        Asset::Url { url } => Ok(url.clone()),
        Asset::Inline { bytes } => Ok(format!(
            "data:{};base64,{}",
            image
                .media_type
                .as_deref()
                .ok_or_else(|| Error::invalid("inline image requires media_type"))?,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )),
        _ => Err(Error::invalid("resolve image references before rendering")),
    }
}

#[cfg(feature = "images")]
pub(crate) fn image_part(
    block: &ContentBlock<Image>,
    target: AssetDialect,
    warnings: &mut Vec<Warning>,
) -> Result<g::Part> {
    use crate::provider_metadata::{PartMetadata, native_part};
    #[derive(Deserialize, Default)]
    struct Envelope {
        #[serde(default)]
        native: Option<PartMetadata>,
    }
    let native = block
        .provider_metadata
        .get(&Vendor::Gemini)
        .map(|m| {
            serde_json::from_value::<Envelope>(serde_json::Value::Object(m.clone()))
                .map_err(Error::invalid)
        })
        .transpose()?
        .unwrap_or_default()
        .native
        .unwrap_or_default();
    let mut value = native_part(native.fields);
    let portable = part(
        &block.content.source,
        block.content.media_type.as_deref(),
        target,
        MediaKind::Image,
    )?;
    value.inline_data = portable.inline_data;
    value.file_data = portable.file_data;
    if let (Some(blob), Some(meta)) = (value.inline_data.as_mut(), native.inline_data) {
        blob.rest = meta.rest;
        blob.display_name = meta.display_name;
    }
    if let (Some(file), Some(meta)) = (value.file_data.as_mut(), native.file_data) {
        file.rest = meta.rest;
        file.display_name = meta.display_name;
    }
    use g::MediaResolutionLevel as L;
    let level = match &block.content.detail {
        Detail::Low => Some(L::Low),
        Detail::Medium => Some(L::Medium),
        Detail::High => Some(L::High),
        Detail::Original => Some(L::UltraHigh),
        Detail::Auto => None,
        Detail::Unknown(_) => {
            warn(warnings, "images.detail", "Unknown image detail is omitted");
            None
        }
    };
    if let Some(level) = level {
        value
            .media_resolution
            .get_or_insert_with(|| g::MediaResolution::builder().build())
            .level = Some(level);
    }
    if block.cache.is_some() {
        warn(
            warnings,
            "images.cache",
            "Image operations have no per-block cache breakpoint",
        );
    }
    for vendor in block.provider_metadata.keys() {
        if *vendor != Vendor::Gemini {
            compatibility(
                warnings,
                "images.provider_metadata",
                "Foreign image metadata is omitted",
            );
        }
    }
    Ok(value)
}
