use super::*;
use base64::Engine as _;

#[derive(Clone)]
pub(crate) enum Media {
    Image {
        url: String,
        detail: Option<source::ImageDetail>,
    },
    Audio {
        url: String,
    },
}

pub(super) fn media(input: &source::GuardianRequestBody) -> Result<Vec<Media>, TransformError> {
    let mut output = Vec::new();
    for entry in &input.input {
        let add_output = |body: &source::FunctionCallOutputBody, output: &mut Vec<Media>| {
            if let source::FunctionCallOutputBody::ContentItems(items) = body {
                for item in items {
                    match item {
                        source::FunctionCallOutputContentItem::InputImage(v) => {
                            output.push(Media::Image {
                                url: v.image_url.clone(),
                                detail: v.detail.clone(),
                            })
                        }
                        source::FunctionCallOutputContentItem::InputAudio(v) => {
                            output.push(Media::Audio {
                                url: v.audio_url.clone(),
                            })
                        }
                        _ => {}
                    }
                }
            }
        };
        match entry {
            source::ClientResponseItem::Message(v) => {
                for item in &v.content {
                    match item {
                        source::ContentItem::InputImage(x) => output.push(Media::Image {
                            url: x.image_url.clone(),
                            detail: x.detail.clone(),
                        }),
                        source::ContentItem::InputAudio(x) => output.push(Media::Audio {
                            url: x.audio_url.clone(),
                        }),
                        _ => {}
                    }
                }
            }
            source::ClientResponseItem::FunctionCallOutput(v) => add_output(&v.output, &mut output),
            source::ClientResponseItem::CustomToolCallOutput(v) => {
                add_output(&v.output, &mut output)
            }
            source::ClientResponseItem::ImageGenerationCall(v) => {
                let bytes = STANDARD.decode(&v.result).map_err(|e| {
                    TransformError::shape("guardian.image_generation.result", e.to_string())
                })?;
                let info = crate::transform::images::inspect_image(&bytes).map_err(|e| {
                    TransformError::shape("guardian.image_generation.result", e.detail())
                })?;
                output.push(Media::Image {
                    url: format!("data:{};base64,{}", info.mime(), v.result),
                    detail: None,
                });
            }
            _ => {}
        }
    }
    Ok(output)
}

pub(super) fn data_uri(value: &str) -> Result<(String, String), TransformError> {
    let (mime, data) = value
        .strip_prefix("data:")
        .and_then(|v| v.split_once(";base64,"))
        .ok_or_else(|| {
            TransformError::missing_metadata(
                "guardian media requires a resolved data URI with MIME",
            )
        })?;
    STANDARD
        .decode(data)
        .map_err(|e| TransformError::shape("guardian.media", e.to_string()))?;
    Ok((mime.into(), data.into()))
}

pub(super) fn detail(value: &Option<source::ImageDetail>) -> r::ImageDetail {
    match value {
        None | Some(source::ImageDetail::Auto) => r::ImageDetail::Auto,
        Some(source::ImageDetail::Low) => r::ImageDetail::Low,
        Some(source::ImageDetail::High) => r::ImageDetail::High,
        Some(source::ImageDetail::Original) => r::ImageDetail::Original,
    }
}

pub(super) fn validate_media(
    target: &GuardianDialectRequest,
    media: &[Media],
) -> Result<(), TransformError> {
    for part in media {
        match part {
            Media::Image {
                detail: selector, ..
            } => {
                if matches!(
                    target,
                    GuardianDialectRequest::Claude(_) | GuardianDialectRequest::Gemini(_)
                ) && detail(selector) != r::ImageDetail::Auto
                {
                    return Err(TransformError::unsupported(
                        "guardian.image.detail",
                        "selected target requires auto image detail",
                    ));
                }
                if matches!(target, GuardianDialectRequest::OpenAiChat(_))
                    && detail(selector) == r::ImageDetail::Original
                {
                    return Err(TransformError::unsupported(
                        "guardian.image.detail",
                        "Chat lacks original image detail",
                    ));
                }
            }
            Media::Audio { .. }
                if matches!(
                    target,
                    GuardianDialectRequest::Claude(_) | GuardianDialectRequest::OpenAiResponses(_)
                ) =>
            {
                return Err(TransformError::unsupported(
                    "guardian.audio",
                    "selected target requires an audio transcription capability",
                ));
            }
            _ => {}
        }
        if part.url().starts_with("data:") {
            let (mime, data) = data_uri(part.url())?;
            let bytes = STANDARD
                .decode(data)
                .map_err(|e| TransformError::shape("guardian.media", e.to_string()))?;
            part.validate_bytes(&mime, &bytes)?;
            if matches!(
                (target, part),
                (GuardianDialectRequest::OpenAiChat(_), Media::Audio { .. })
            ) && !matches!(mime.as_str(), "audio/wav" | "audio/mp3" | "audio/mpeg")
            {
                return Err(TransformError::unsupported(
                    "guardian.audio.mime",
                    "Chat supports WAV and MP3",
                ));
            }
        } else {
            let uri: http::Uri = part
                .url()
                .parse()
                .map_err(|_| TransformError::shape("guardian.media.url", "HTTP(S) URL required"))?;
            if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.authority().is_none() {
                return Err(TransformError::shape(
                    "guardian.media.url",
                    "HTTP(S) URL required",
                ));
            }
        }
    }
    Ok(())
}

impl Media {
    pub(crate) fn url(&self) -> &str {
        match self {
            Self::Image { url, .. } | Self::Audio { url } => url,
        }
    }
    pub(crate) fn set_url(&mut self, value: String) {
        match self {
            Self::Image { url, .. } | Self::Audio { url } => *url = value,
        }
    }
    pub(crate) fn needs_resolution(&self, target: GuardianTarget) -> bool {
        !self.url().starts_with("data:")
            && (target == GuardianTarget::Gemini
                || matches!(
                    (target, self),
                    (GuardianTarget::OpenAiChat, Self::Audio { .. })
                ))
    }
    pub(crate) fn validate_bytes(&self, mime: &str, bytes: &[u8]) -> Result<(), TransformError> {
        match self {
            Self::Image { .. } => {
                if mime == "image/gif" {
                    if bytes.len() < 13
                        || !matches!(&bytes[..6], b"GIF87a" | b"GIF89a")
                        || u16::from_le_bytes([bytes[6], bytes[7]]) == 0
                        || u16::from_le_bytes([bytes[8], bytes[9]]) == 0
                    {
                        return Err(TransformError::shape(
                            "guardian.media",
                            "invalid GIF header",
                        ));
                    }
                } else {
                    let actual = crate::transform::images::inspect_image(bytes)
                        .map_err(|e| TransformError::shape("guardian.media", e.detail()))?;
                    if actual.mime() != mime {
                        return Err(TransformError::shape(
                            "guardian.media.mime",
                            "MIME contradicts image container",
                        ));
                    }
                }
            }
            Self::Audio { .. } => {
                if !mime.starts_with("audio/") || bytes.is_empty() {
                    return Err(TransformError::shape(
                        "guardian.media.mime",
                        "nonempty audio payload and audio MIME required",
                    ));
                }
                if mime == "audio/wav"
                    && (bytes.len() < 12
                        || &bytes[..4] != b"RIFF"
                        || &bytes[8..12] != b"WAVE"
                        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8
                            != bytes.len())
                {
                    return Err(TransformError::shape(
                        "guardian.media",
                        "invalid WAV container",
                    ));
                }
                if mime == "audio/wav" {
                    let mut at = 12usize;
                    let mut format = false;
                    let mut data = false;
                    while at < bytes.len() {
                        if bytes.len() - at < 8 {
                            return Err(TransformError::shape(
                                "guardian.media",
                                "truncated WAV chunk",
                            ));
                        }
                        let n =
                            u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
                        let end = at
                            .checked_add(8)
                            .and_then(|v| v.checked_add(n))
                            .filter(|v| *v <= bytes.len())
                            .ok_or_else(|| {
                                TransformError::shape("guardian.media", "truncated WAV payload")
                            })?;
                        if &bytes[at..at + 4] == b"fmt " {
                            format |= n >= 16;
                        }
                        if &bytes[at..at + 4] == b"data" {
                            data |= n > 0;
                        }
                        at = end.checked_add(n % 2).ok_or_else(|| {
                            TransformError::shape("guardian.media", "WAV size overflow")
                        })?;
                    }
                    if !format || !data || at != bytes.len() {
                        return Err(TransformError::shape(
                            "guardian.media",
                            "WAV requires format and nonempty data chunks",
                        ));
                    }
                }
                if matches!(mime, "audio/mpeg" | "audio/mp3")
                    && !bytes.starts_with(b"ID3")
                    && !(bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
                {
                    return Err(TransformError::shape(
                        "guardian.media",
                        "invalid MP3 framing",
                    ));
                }
            }
        }
        Ok(())
    }
}
