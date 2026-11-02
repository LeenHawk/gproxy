//! Application references are resolved by the caller. No file/network I/O,
//! uploads, format conversion, or vendor-specific asset preparation lives here.
use crate::*;
use std::future::Future;

pub enum ResolvedAsset {
    Bytes { bytes: Vec<u8>, media_type: String },
    Url { url: String },
}

impl GenerateRequest {
    pub async fn resolve_assets<F, Fut>(&mut self, mut resolve: F) -> Result<()>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<ResolvedAsset>>,
    {
        for message in &mut self.messages {
            for block in &mut message.content {
                match &mut block.content {
                    Content::Image(image) => {
                        resolve_media(&mut image.source, &mut image.media_type, &mut resolve)
                            .await?
                    }
                    Content::Document(document) => resolve_document(document, &mut resolve).await?,
                    Content::File(file) => {
                        resolve_media(&mut file.source, &mut file.media_type, &mut resolve).await?
                    }
                    Content::Audio(audio) => {
                        resolve_source(&mut audio.source, &mut resolve).await?;
                    }
                    Content::Video(video) => {
                        resolve_media(&mut video.source, &mut video.media_type, &mut resolve)
                            .await?
                    }
                    Content::ToolResult(result) => {
                        if let ToolOutput::Blocks(blocks) = &mut result.output {
                            for block in blocks {
                                match &mut block.content {
                                    ToolResultContent::Image(image) => {
                                        resolve_media(
                                            &mut image.source,
                                            &mut image.media_type,
                                            &mut resolve,
                                        )
                                        .await?
                                    }
                                    ToolResultContent::Document(document) => {
                                        resolve_document(document, &mut resolve).await?
                                    }
                                    ToolResultContent::Text(_) | ToolResultContent::Unknown(_) => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
async fn resolve_document<F, Fut>(document: &mut Document, resolve: &mut F) -> Result<()>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    match &mut document.source {
        DocumentSource::Asset { source } => {
            resolve_media(source, &mut document.media_type, resolve).await?
        }
        DocumentSource::Chunks { chunks } => {
            for chunk in chunks {
                if let ChunkContent::Image(image) = &mut chunk.content {
                    resolve_media(&mut image.source, &mut image.media_type, resolve).await?;
                }
            }
        }
        DocumentSource::Text { .. } | DocumentSource::Unknown(_) => {}
    }
    Ok(())
}
pub(crate) async fn resolve_media<F, Fut>(
    source: &mut Asset,
    mime: &mut Option<String>,
    resolve: &mut F,
) -> Result<()>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    if let Some(resolved_mime) = resolve_source(source, resolve).await? {
        *mime = Some(resolved_mime);
    }
    Ok(())
}
async fn resolve_source<F, Fut>(source: &mut Asset, resolve: &mut F) -> Result<Option<String>>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<ResolvedAsset>>,
{
    let Asset::Reference { reference } = source else {
        return Ok(None);
    };
    let (resolved, mime) = match resolve(reference.clone()).await? {
        ResolvedAsset::Bytes { bytes, media_type } => (Asset::Inline { bytes }, Some(media_type)),
        ResolvedAsset::Url { url } => (Asset::Url { url }, None),
    };
    *source = resolved;
    Ok(mime)
}
