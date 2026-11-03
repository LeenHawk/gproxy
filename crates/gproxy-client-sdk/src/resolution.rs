//! Request-scoped asset preparation. The original request retains references.
//! Resolvers own storage/signing; the SDK does no file I/O, URL fetch or upload.
use crate::*;
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AssetLocation {
    Message,
    ToolResult,
    DocumentChunk,
    Embedding,
}
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResolveAsset {
    pub reference: String,
    pub target: AssetDialect,
    pub kind: MediaKind,
    pub location: AssetLocation,
    pub media_type: Option<String>,
    /// Already narrowed for this content envelope and known MIME type.
    pub capability: MediaCapability,
    pub preferred: DeliveryMethod,
}
#[derive(Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ResolvedAsset {
    Bytes {
        bytes: Vec<u8>,
        media_type: String,
    },
    Url {
        url: String,
        media_type: Option<String>,
        /// Optional storage-object size, supplied by the caller. Never fetched.
        byte_len: Option<u64>,
    },
}

/// Deliberately neither Serialize nor Deref<Target=GenerateRequest>. Keep the
/// original request/Conversation as history; this snapshot contains signed URLs.
/// Sending or rendering consumes the snapshot, so each next send needs preparation.
///
/// ```compile_fail
/// use gproxy_client_sdk::PreparedGenerateRequest;
/// fn persist(prepared: PreparedGenerateRequest) {
///     let _ = serde_json::to_string(&prepared); // no Serialize implementation
/// }
/// ```
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub struct PreparedGenerateRequest {
    pub(crate) request: GenerateRequest,
    pub(crate) warnings: Vec<Warning>,
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
impl PreparedGenerateRequest {
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
    pub fn render(self, streaming: bool) -> Result<Rendered<serde_json::Value>> {
        let mut rendered = crate::dialects::render(&self.request, streaming)?;
        let mut warnings = self.warnings;
        warnings.extend(rendered.warnings);
        rendered.warnings = warnings;
        Ok(rendered)
    }
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
impl GenerateRequest {
    /// Prepare a fresh send-only snapshot. Call again for every send to obtain
    /// fresh presigned URLs. No resolved value is written into this request.
    pub async fn resolve_assets<F, Fut>(&self, mut resolve: F) -> Result<PreparedGenerateRequest>
    where
        F: FnMut(ResolveAsset) -> Fut,
        Fut: Future<Output = Result<ResolvedAsset>>,
    {
        let mut request = self.clone();
        let target = request.model.dialect.into();
        let mut warnings = Vec::new();
        for (mi, message) in request.messages.iter_mut().enumerate() {
            for (bi, block) in message.content.iter_mut().enumerate() {
                let path = format!("messages[{mi}].content[{bi}]");
                prepare_block(
                    &mut block.content,
                    &message.role,
                    target,
                    &path,
                    &mut resolve,
                    &mut warnings,
                )
                .await?;
            }
        }
        Ok(PreparedGenerateRequest { request, warnings })
    }
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
async fn prepare_block<F, Fut>(
    content: &mut Content,
    role: &Role,
    target: AssetDialect,
    path: &str,
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
            prepare_image(image, target, location, path, resolve, warnings).await?
        }
        Content::Document(doc) => {
            prepare_document(doc, target, location, path, resolve, warnings).await?
        }
        Content::File(file) => {
            prepare_media(
                &mut file.source,
                &mut file.media_type,
                target,
                MediaKind::File,
                location,
                path,
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
pub(crate) fn audio_mime(format: &AudioFormat) -> Result<&str> {
    match format {
        AudioFormat::Encoded { media_type } | AudioFormat::Pcm { media_type, .. } => Ok(media_type),
        AudioFormat::Unknown(_) => Err(Error::invalid("unknown audio format")),
    }
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
pub(crate) async fn prepare_image<F, Fut>(
    image: &mut Image,
    target: AssetDialect,
    location: AssetLocation,
    path: &str,
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
        resolve,
        warnings,
    )
    .await
}
pub(crate) fn capability_at(
    target: AssetDialect,
    kind: MediaKind,
    location: AssetLocation,
) -> MediaCapability {
    let mut cap = media_capability(target, kind);
    if location == AssetLocation::ToolResult {
        match target {
            // protocol gemini::FunctionResponsePart has inline_data only.
            // Generating content.md:3550-3562 documents this envelope.
            AssetDialect::Gemini => cap.url = None,
            // Chat tool content is text-only (chat::content::TextContent).
            AssetDialect::OpenaiChat => {
                cap.inline = None;
                cap.url = None;
            }
            _ => {}
        }
    }
    cap
}
#[allow(clippy::too_many_arguments)] // One media slot plus its render context; no retained resolver state.
async fn prepare_media<F, Fut>(
    source: &mut Asset,
    mime: &mut Option<String>,
    target: AssetDialect,
    kind: MediaKind,
    location: AssetLocation,
    path: &str,
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
        .preferred()
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
            "Resolver returned inline bytes instead of the preferred URL",
        ));
    }
    *source = value;
    Ok(())
}
pub(crate) fn check_asset(
    target: AssetDialect,
    kind: MediaKind,
    location: AssetLocation,
    source: &Asset,
    mime: Option<&str>,
    size: Option<u64>,
) -> Result<()> {
    let cap = capability_at(target, kind, location);
    let (constraints, size) = match source {
        Asset::Inline { bytes } => {
            if mime.is_none() {
                return Err(Error::invalid("inline media requires media_type"));
            }
            (cap.inline, size.or(Some(bytes.len() as u64)))
        }
        Asset::Url { .. } => (cap.url, size),
        // Native history replay remains supported by the typed renderers, but
        // file IDs are not a delivery method offered by asset preparation.
        Asset::ProviderFile { .. } => {
            return Err(Error::invalid(
                "asset preparation accepts inline bytes or URLs; replace provider file IDs with application references",
            ));
        }
        Asset::Unknown(_) => return Err(Error::invalid("unknown media source")),
        Asset::Reference { .. } => return Ok(()), // Typed renderers report an unresolved reference.
    };
    constraints
        .ok_or_else(|| {
            Error::invalid(
                "this dialect/content envelope does not accept this media delivery method",
            )
        })?
        .check(mime, size)
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub(crate) fn validate_request(request: &GenerateRequest) -> Result<()> {
    let target = request.model.dialect.into();
    for message in &request.messages {
        for block in &message.content {
            validate_content(&block.content, &message.role, target)?;
        }
    }
    Ok(())
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
fn validate_content(content: &Content, role: &Role, target: AssetDialect) -> Result<()> {
    let location = AssetLocation::Message;
    match content {
        Content::Image(i) => validate_source(
            target,
            MediaKind::Image,
            location,
            &i.source,
            i.media_type.as_deref(),
        ),
        Content::Document(d) => validate_document(d, target, location),
        Content::File(f) => validate_source(
            target,
            MediaKind::File,
            location,
            &f.source,
            f.media_type.as_deref(),
        ),
        Content::Video(v) => validate_source(
            target,
            MediaKind::Video,
            location,
            &v.source,
            v.media_type.as_deref(),
        ),
        Content::Audio(_) if target == AssetDialect::OpenaiChat && *role == Role::Assistant => {
            Ok(())
        }
        Content::Audio(a) => validate_source(
            target,
            MediaKind::Audio,
            location,
            &a.source,
            Some(audio_mime(&a.format)?),
        ),
        Content::ToolResult(r) => {
            if let ToolOutput::Blocks(blocks) = &r.output {
                for block in blocks {
                    match &block.content {
                        ToolResultContent::Image(i) => validate_source(
                            target,
                            MediaKind::Image,
                            AssetLocation::ToolResult,
                            &i.source,
                            i.media_type.as_deref(),
                        )?,
                        ToolResultContent::Document(d) => {
                            validate_document(d, target, AssetLocation::ToolResult)?
                        }
                        _ => {}
                    }
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
fn validate_source(
    target: AssetDialect,
    kind: MediaKind,
    location: AssetLocation,
    source: &Asset,
    mime: Option<&str>,
) -> Result<()> {
    // Already-native file IDs are replayed by the existing typed dialect mapping.
    if matches!(source, Asset::ProviderFile { .. }) {
        return Ok(());
    }
    check_asset(target, kind, location, source, mime, None)
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
fn validate_document(doc: &Document, target: AssetDialect, location: AssetLocation) -> Result<()> {
    match &doc.source {
        DocumentSource::Asset { source } => validate_source(
            target,
            MediaKind::Document,
            location,
            source,
            doc.media_type.as_deref(),
        ),
        DocumentSource::Text { text } => capability_at(target, MediaKind::Document, location)
            .inline
            .ok_or_else(|| Error::invalid("document input is unsupported"))?
            .check(Some("text/plain"), Some(text.len() as u64)),
        DocumentSource::Chunks { chunks } => {
            for chunk in chunks {
                if let ChunkContent::Image(i) = &chunk.content {
                    validate_source(
                        target,
                        MediaKind::Image,
                        AssetLocation::DocumentChunk,
                        &i.source,
                        i.media_type.as_deref(),
                    )?;
                }
            }
            Ok(())
        }
        DocumentSource::Unknown(_) => Err(Error::invalid("unknown document source")),
    }
}

/// Count actual JSON bytes without retaining a second copy of large media.
#[cfg(feature = "claude")]
pub(crate) fn check_claude_request(body: &impl serde::Serialize) -> Result<()> {
    struct Count {
        bytes: u64,
    }
    impl std::io::Write for Count {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.bytes += data.len() as u64;
            // upstream_docs/claude/docs/API overview.md:111: 32 MB request body.
            if self.bytes > crate::asset_capabilities::CLAUDE_MAX_REQUEST_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Claude native request exceeds 32 MB",
                ));
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Count { bytes: 0 }, body).map_err(Error::invalid)
}
