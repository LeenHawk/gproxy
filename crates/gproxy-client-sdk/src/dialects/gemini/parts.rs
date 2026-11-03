use super::*;
pub(super) fn response_part(
    block: &ToolResultBlock,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
) -> Result<g::FunctionResponsePart> {
    #[derive(Deserialize)]
    struct Envelope {
        function_result_part: Option<ResponsePartMetadata>,
    }
    let meta =
        read_metadata::<Envelope>(block, Vendor::Gemini)?.and_then(|v| v.function_result_part);
    media::check_file_vendor(source, Vendor::Gemini)?;
    let mut part = g::FunctionResponsePart::builder().build();
    match source {
        Asset::Inline { bytes } => {
            part.inline_data = Some(
                g::Blob::builder(
                    media::mime(mime, source)?.into(),
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                )
                .build(),
            )
        }
        Asset::Url { .. } | Asset::ProviderFile { .. } => {
            return Err(Error::invalid(
                "Gemini function response media requires inline bytes",
            ));
        }
        Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "prepare application references before rendering tool output",
            ));
        }
    }
    if let (Some(name), Some(blob)) = (filename, part.inline_data.as_mut()) {
        blob.display_name = Some(name.into());
    }
    if let Some(meta) = meta {
        part.rest = meta.fields.rest;
        if let (Some(native), Some(blob)) = (meta.inline_data, part.inline_data.as_mut()) {
            blob.rest = native.rest;
            if native.display_name.is_some() {
                blob.display_name = native.display_name;
            }
        }
    }
    Ok(part)
}

pub(super) fn set_detail(part: &mut g::Part, detail: &Detail) {
    if let Some(mapped) = media::gemini_detail(detail) {
        if let Some(native) = &mut part.media_resolution {
            native.level = mapped.level;
        } else {
            part.media_resolution = Some(mapped);
        }
    }
}

pub(super) fn render_asset(
    part: &mut g::Part,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
    inline: Option<MediaMetadata>,
    file: Option<MediaMetadata>,
) -> Result<()> {
    match source {
        Asset::Inline { bytes } => {
            let mut value = g::Blob::builder(
                media::mime(mime, source)?.into(),
                base64::engine::general_purpose::STANDARD.encode(bytes),
            )
            .build();
            if let Some(native) = inline {
                value.rest = native.rest;
                value.display_name = native.display_name;
            }
            if let Some(name) = filename {
                value.display_name = Some(name.into());
            }
            part.inline_data = Some(value);
            part.file_data = None;
        }
        Asset::Url { url } | Asset::ProviderFile { id: url, .. } => {
            let mut value = g::FileData::builder(url.clone()).build();
            if let Some(native) = file {
                value.rest = native.rest;
                value.display_name = native.display_name;
                value.mime_type = native.mime_type;
            }
            if let Some(mime) = mime {
                value.mime_type = Some(mime.into());
            }
            if let Some(name) = filename {
                value.display_name = Some(name.into());
            }
            part.file_data = Some(value);
            part.inline_data = None;
        }
        Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "prepare application assets before rendering",
            ));
        }
    }
    Ok(())
}

pub(super) fn duration_field(
    original: Option<String>,
    value: Option<f64>,
) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    if !value.is_finite() || value < 0.0 {
        return Err(Error::invalid(
            "video offsets must be finite and nonnegative",
        ));
    }
    if let Some(original) = original
        && parse_duration(&original)? == value
    {
        return Ok(Some(original));
    }
    Ok(Some(format!("{value:.9}s")))
}

pub(super) fn parse_duration(value: &str) -> Result<f64> {
    value
        .strip_suffix('s')
        .ok_or_else(|| Error::invalid("Gemini offsets require seconds"))?
        .parse()
        .map_err(Error::invalid)
}

pub(super) fn parse_blob(
    blob: g::Blob,
    detail: Detail,
    transcription: Option<g::audio::AudioTranscription>,
) -> Result<(Content, MediaMetadata, Option<AudioMetadata>)> {
    let g::Blob {
        mime_type,
        data,
        display_name,
        rest,
    } = blob;
    let source = decode_inline(&data)?;
    let base = mime_type.split(';').next().unwrap_or(&mime_type);
    let mut native = MediaMetadata {
        mime_type: None,
        display_name: display_name.clone(),
        rest,
    };
    let mut audio_meta = None;
    let content = if base.starts_with("image/") {
        native.display_name = display_name;
        Content::Image(Image {
            source,
            media_type: Some(mime_type),
            detail,
        })
    } else if base.starts_with("audio/") {
        let format = media::parse_audio_format(&mime_type)?;
        if media::pcm_mime(&format)? != mime_type {
            native.mime_type = Some(mime_type);
        }
        let transcript = transcription.map(|mut value| {
            let text = std::mem::take(&mut value.text);
            audio_meta = Some(audio_metadata(value));
            text
        });
        Content::Audio(Audio {
            source,
            format,
            transcript,
        })
    } else if base.starts_with("video/") {
        Content::Video(Video {
            source,
            media_type: Some(mime_type),
            clip: None,
        })
    } else if matches!(base, "application/pdf" | "text/plain") {
        Content::Document(Document {
            source: DocumentSource::Asset { source },
            media_type: Some(mime_type),
            title: display_name,
            context: None,
            citations: None,
            detail,
        })
    } else {
        Content::File(File {
            source,
            media_type: Some(mime_type),
            filename: display_name,
        })
    };
    if matches!(content, Content::Document(_) | Content::File(_)) {
        native.display_name = None;
    }
    Ok((content, native, audio_meta))
}

pub(super) fn parse_file(file: g::FileData, detail: Detail) -> Result<(Content, MediaMetadata)> {
    let g::FileData {
        file_uri,
        mime_type,
        display_name,
        rest,
    } = file;
    let source = if file_uri.starts_with("https://generativelanguage.googleapis.com/v1beta/files/")
        || file_uri.starts_with("files/")
    {
        Asset::ProviderFile {
            vendor: Vendor::Gemini,
            id: file_uri,
        }
    } else {
        Asset::Url { url: file_uri }
    };
    let base = mime_type.as_deref().and_then(|s| s.split(';').next());
    let mut native = MediaMetadata {
        mime_type: None,
        display_name: display_name.clone(),
        rest,
    };
    let content = if base.is_some_and(|s| s.starts_with("image/")) {
        Content::Image(Image {
            source,
            media_type: mime_type,
            detail,
        })
    } else if base.is_some_and(|s| s.starts_with("audio/")) {
        let format = media::parse_audio_format(mime_type.as_deref().unwrap_or_default())?;
        if let Some(mime) = mime_type
            && media::pcm_mime(&format)? != mime
        {
            native.mime_type = Some(mime);
        }
        Content::Audio(Audio {
            source,
            format,
            transcript: None,
        })
    } else if base.is_some_and(|s| s.starts_with("video/")) {
        Content::Video(Video {
            source,
            media_type: mime_type,
            clip: None,
        })
    } else if matches!(base, Some("application/pdf" | "text/plain")) {
        Content::Document(Document {
            source: DocumentSource::Asset { source },
            media_type: mime_type,
            title: display_name,
            context: None,
            citations: None,
            detail,
        })
    } else {
        Content::File(File {
            source,
            media_type: mime_type,
            filename: display_name,
        })
    };
    if matches!(content, Content::Document(_) | Content::File(_)) {
        native.display_name = None;
    }
    Ok((content, native))
}

pub(super) fn parse_video(value: Option<&g::VideoMetadata>) -> Result<Option<VideoClip>> {
    value
        .map(|v| {
            Ok(VideoClip {
                start_seconds: v.start_offset.as_deref().map(parse_duration).transpose()?,
                end_seconds: v.end_offset.as_deref().map(parse_duration).transpose()?,
                fps: v.fps,
            })
        })
        .transpose()
}

pub(super) fn parse_response_part(part: g::FunctionResponsePart) -> Result<ToolResultBlock> {
    let mut fields = part;
    let (content, inline) = if let Some(blob) = fields.inline_data.take() {
        let (content, inline, _) = parse_blob(blob, Detail::Auto, None)?;
        (content, Some(inline))
    } else {
        return Err(Error::invalid("function-response media part has no source"));
    };
    let content = match content {
        Content::Image(v) => ToolResultContent::Image(v),
        Content::Document(v) => ToolResultContent::Document(v),
        _ => {
            return Err(Error::invalid(
                "tool output supports text, images and documents",
            ));
        }
    };
    #[derive(Serialize)]
    struct Envelope {
        function_result_part: ResponsePartMetadata,
    }
    with_metadata(
        content,
        Vendor::Gemini,
        Envelope {
            function_result_part: ResponsePartMetadata {
                inline_data: inline,
                fields: ResponseFields { rest: fields.rest },
            },
        },
    )
}

pub(crate) fn merge_transcription(block: &mut Block, part: &g::Part) -> Result<Option<String>> {
    let Content::Audio(audio) = &mut block.content else {
        return Ok(None);
    };
    let Some(transcript) = &part.audio_transcription else {
        return Ok(None);
    };
    if part.inline_data.is_some() || part.file_data.is_some() || part.text.is_some() {
        return Ok(None);
    }
    // The metadata records boundaries and extensions, not a second transcript.
    let mut meta = match block.provider_metadata.get(&Vendor::Gemini) {
        Some(rest) => serde_json::from_value::<Native<PartMetadata>>(Value::Object(rest.clone()))
            .map_err(Error::invalid)?
            .native
            .unwrap_or_default(),
        None => PartMetadata::default(),
    };
    let text = audio.transcript.get_or_insert_with(String::new);
    if meta.primary_transcript_chars.is_none() {
        meta.primary_transcript_chars = Some(text.chars().count());
    }
    text.push_str(&transcript.text);
    let fields = part_fields(part.clone());
    let native = audio_metadata(transcript.clone());
    meta.trailing_transcripts.push(TranscriptMetadata {
        fields,
        transcription: native,
        chars: transcript.text.chars().count(),
    });
    block
        .provider_metadata
        .entry(Vendor::Gemini)
        .or_default()
        .extend(encode_rest(Native { native: Some(meta) })?);
    Ok(Some(transcript.text.clone()))
}

pub(super) fn trailing_transcripts(block: &Block) -> Result<Vec<g::Part>> {
    let Content::Audio(audio) = &block.content else {
        return Ok(Vec::new());
    };
    let Some(meta) = read_native::<PartMetadata>(block, Vendor::Gemini)? else {
        return Ok(Vec::new());
    };
    let mut chars = audio
        .transcript
        .as_deref()
        .unwrap_or_default()
        .chars()
        .skip(meta.primary_transcript_chars.unwrap_or(0));
    let count = meta.trailing_transcripts.len();
    let mut parts = Vec::with_capacity(count);
    for (index, meta) in meta.trailing_transcripts.into_iter().enumerate() {
        let text = if index + 1 == count {
            chars.by_ref().collect()
        } else {
            chars.by_ref().take(meta.chars).collect()
        };
        let mut part = native_part(meta.fields);
        part.audio_transcription = Some(transcription(text, meta.transcription));
        parts.push(part);
    }
    Ok(parts)
}
