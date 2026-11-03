#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use super::validation::audio_mime;
use super::validation::{capability_at, check_asset};
use crate::*;
use std::future::Future;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub(super) async fn prepare_block<F, Fut>(
    content: &mut Content,
    role: &Role,
    target: AssetDialect,
    path: &str,
    mode: DeliveryMode,
    resolve: &mut F,
    warnings: &mut Vec<Warning>,
) -> Result<()>
where
    F: FnMut(ResolveAsset) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    let location = AssetLocation::Message;
    match content {
        Content::Image(image) => {
            prepare_image(image, target, location, path, mode, resolve, warnings).await?
        }
        Content::Document(doc) => {
            prepare_document(doc, target, location, path, mode, resolve, warnings).await?
        }
        Content::File(file) => {
            prepare_media(
                &mut file.source,
                &mut file.media_type,
                target,
                MediaKind::File,
                location,
                path,
                mode,
                resolve,
                warnings,
            )
            .await?
        }
        // Chat assistant audio is replayed by its native audio ID or transcript;
        // its source bytes are not sent as input_audio (which only accepts WAV/MP3).
        Content::Audio(_) if target == AssetDialect::OpenaiChat && *role == Role::Assistant => {}
        Content::Audio(audio) => {
            let mut mime = Some(audio_mime(&audio.format)?.to_owned());
            prepare_media(
                &mut audio.source,
                &mut mime,
                target,
                MediaKind::Audio,
                location,
                path,
                mode,
                resolve,
                warnings,
            )
            .await?;
        }
        Content::Video(video) => {
            prepare_media(
                &mut video.source,
                &mut video.media_type,
                target,
                MediaKind::Video,
                location,
                path,
                mode,
                resolve,
                warnings,
            )
            .await?
        }
        Content::ToolResult(result) => {
            if let ToolOutput::Blocks(blocks) = &mut result.output {
                for (i, block) in blocks.iter_mut().enumerate() {
                    let path = format!("{path}.output[{i}]");
                    match &mut block.content {
                        ToolResultContent::Image(image) => {
                            prepare_image(
                                image,
                                target,
                                AssetLocation::ToolResult,
                                &path,
                                mode,
                                resolve,
                                warnings,
                            )
                            .await?
                        }
                        ToolResultContent::Document(doc) => {
                            prepare_document(
                                doc,
                                target,
                                AssetLocation::ToolResult,
                                &path,
                                mode,
                                resolve,
                                warnings,
                            )
                            .await?
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
async fn prepare_document<F, Fut>(
    doc: &mut Document,
    target: AssetDialect,
    location: AssetLocation,
    path: &str,
    mode: DeliveryMode,
    resolve: &mut F,
    warnings: &mut Vec<Warning>,
) -> Result<()>
where
    F: FnMut(ResolveAsset) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    match &mut doc.source {
        DocumentSource::Asset { source } => {
            prepare_media(
                source,
                &mut doc.media_type,
                target,
                MediaKind::Document,
                location,
                path,
                mode,
                resolve,
                warnings,
            )
            .await?
        }
        DocumentSource::Text { text } => {
            capability_at(target, MediaKind::Document, location)
                .inline
                .ok_or_else(|| Error::invalid("document input is unsupported"))?
                .check(Some("text/plain"), Some(text.len() as u64))?;
        }
        DocumentSource::Chunks { chunks } => {
            if target != AssetDialect::Claude {
                return Err(Error::invalid(
                    "custom document chunks require Claude Messages",
                ));
            }
            for (i, chunk) in chunks.iter_mut().enumerate() {
                if let ChunkContent::Image(image) = &mut chunk.content {
                    prepare_image(
                        image,
                        target,
                        AssetLocation::DocumentChunk,
                        &format!("{path}.chunks[{i}]"),
                        mode,
                        resolve,
                        warnings,
                    )
                    .await?;
                }
            }
        }
        DocumentSource::Unknown(_) => return Err(Error::invalid("unknown document source")),
    }
    Ok(())
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings"
))]
pub(crate) async fn prepare_image<F, Fut>(
    image: &mut Image,
    target: AssetDialect,
    location: AssetLocation,
    path: &str,
    mode: DeliveryMode,
    resolve: &mut F,
    warnings: &mut Vec<Warning>,
) -> Result<()>
where
    F: FnMut(ResolveAsset) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    prepare_media(
        &mut image.source,
        &mut image.media_type,
        target,
        MediaKind::Image,
        location,
        path,
        mode,
        resolve,
        warnings,
    )
    .await
}

#[allow(clippy::too_many_arguments)] // One media slot plus its render context; no retained resolver state.
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "images",
    feature = "transcription"
))]
pub(crate) async fn prepare_media<F, Fut>(
    source: &mut Asset,
    mime: &mut Option<String>,
    target: AssetDialect,
    kind: MediaKind,
    location: AssetLocation,
    path: &str,
    mode: DeliveryMode,
    resolve: &mut F,
    warnings: &mut Vec<Warning>,
) -> Result<()>
where
    F: FnMut(ResolveAsset) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    let Asset::Reference { reference } = source else {
        return check_asset(target, kind, location, source, mime.as_deref(), None);
    };
    let cap = capability_at(target, kind, location).for_mime(mime.as_deref());
    let preferred = cap
        .preferred(mode)
        .ok_or_else(|| Error::invalid(format!("{path}: no supported asset delivery method")))?;
    let resolution = resolve(ResolveAsset {
        reference: reference.clone(),
        target,
        kind,
        location,
        media_type: mime.clone(),
        capability: cap,
        preferred,
    })
    .await?;
    let (value, resolved_mime, size, method) = match resolution {
        ResolvedAsset::Bytes { bytes, media_type } => (
            Asset::Inline { bytes },
            Some(media_type),
            None,
            DeliveryMethod::Inline,
        ),
        ResolvedAsset::Url {
            url,
            media_type,
            byte_len,
        } => (
            Asset::Url { url },
            media_type,
            byte_len,
            DeliveryMethod::Url,
        ),
    };
    if let (Some(declared), Some(resolved)) = (mime.as_deref(), resolved_mime.as_deref())
        && declared != resolved
    {
        return Err(Error::invalid(format!(
            "{path}: resolver MIME type differs from the declared media type"
        )));
    }
    if resolved_mime.is_some() {
        *mime = resolved_mime;
    }
    check_asset(target, kind, location, &value, mime.as_deref(), size)?;
    if method != preferred {
        warnings.push(Warning::new(
            WarningKind::Compatibility,
            path,
            match preferred {
                DeliveryMethod::Url => {
                    "Resolver returned inline bytes instead of the preferred URL"
                }
                DeliveryMethod::Inline => {
                    "Resolver returned a URL instead of the preferred inline bytes"
                }
            },
        ));
    }
    *source = value;
    Ok(())
}
