use crate::*;
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
    feature = "gemini",
    feature = "embeddings"
))]
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

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings"
))]
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
            if self.bytes > crate::assets::capabilities::CLAUDE_MAX_REQUEST_BYTES {
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
