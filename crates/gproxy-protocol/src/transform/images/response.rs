use super::request::ImageRequestContext;
use crate::{
    transform::{Report, TransformError, TransformErrorKind},
    wire::{
        DeclaredFields, gemini as g,
        openai::{images as o, responses as r},
    },
};
use base64::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMetadata {
    pub format: o::ImageOutputFormat,
    pub width: u32,
    pub height: u32,
}

impl ImageMetadata {
    pub fn mime(&self) -> &'static str {
        match self.format {
            o::ImageOutputFormat::Png => "image/png",
            o::ImageOutputFormat::Jpeg => "image/jpeg",
            o::ImageOutputFormat::Webp => "image/webp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImagePart {
    pub bytes: bytes::Bytes,
    pub metadata: ImageMetadata,
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageUsageFacts {
    Responses(r::response::ResponseUsage),
    Gemini(g::UsageMetadata),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageCallResult {
    pub image: GeneratedImagePart,
    pub usage: Option<ImageUsageFacts>,
    pub response_id: Option<String>,
    pub model: Option<String>,
    pub report: Report,
}

fn invalid(detail: &str) -> TransformError {
    TransformError::invalid_result("image.output", detail)
}

fn u32be(b: &[u8]) -> u32 {
    u32::from_be_bytes(b.try_into().unwrap())
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}

fn crc32(b: &[u8]) -> u32 {
    let mut c = !0u32;
    for x in b {
        c ^= u32::from(*x);
        for _ in 0..8 {
            c = (c >> 1) ^ ((0u32.wrapping_sub(c & 1)) & 0xedb88320);
        }
    }
    !c
}

/// Container validation and dimensions; this does not decode pixels. PNG CRCs,
/// JPEG marker framing and WebP RIFF framing are checked before publication.
pub fn inspect_image(b: &[u8]) -> Result<ImageMetadata, TransformError> {
    let (format, width, height) = if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut pos = 8usize;
        let mut dimensions = None;
        let mut data = false;
        let mut ended = false;
        while pos < b.len() {
            if b.len() - pos < 12 {
                return Err(invalid("truncated PNG chunk"));
            }
            let len = u32be(&b[pos..pos + 4]) as usize;
            let end = pos
                .checked_add(12)
                .and_then(|p| p.checked_add(len))
                .filter(|p| *p <= b.len())
                .ok_or_else(|| invalid("truncated PNG payload"))?;
            let kind = &b[pos + 4..pos + 8];
            if crc32(&b[pos + 4..end - 4]) != u32be(&b[end - 4..end]) {
                return Err(invalid("invalid PNG CRC"));
            }
            if pos == 8 {
                if kind != b"IHDR" || len != 13 {
                    return Err(invalid("PNG requires IHDR"));
                }
                let width = u32be(&b[pos + 8..pos + 12]);
                let height = u32be(&b[pos + 12..pos + 16]);
                let depth = b[pos + 16];
                let color = b[pos + 17];
                if width > 0x7fffffff
                    || height > 0x7fffffff
                    || !match color {
                        0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
                        2 | 4 | 6 => matches!(depth, 8 | 16),
                        3 => matches!(depth, 1 | 2 | 4 | 8),
                        _ => false,
                    }
                    || b[pos + 18] != 0
                    || b[pos + 19] != 0
                    || b[pos + 20] > 1
                {
                    return Err(invalid("invalid PNG header"));
                }
                dimensions = Some((width, height));
            } else if kind == b"IHDR" {
                return Err(invalid("duplicate PNG header"));
            }
            if kind == b"IDAT" && len > 0 {
                data = true;
            }
            if kind == b"IEND" {
                if len != 0 || end != b.len() {
                    return Err(invalid("invalid PNG end"));
                }
                ended = true;
            }
            pos = end;
        }
        if !data || !ended {
            return Err(invalid("PNG image data or end missing"));
        }
        let (w, h) = dimensions.ok_or_else(|| invalid("PNG dimensions missing"))?;
        (o::ImageOutputFormat::Png, w, h)
    } else if b.starts_with(&[0xff, 0xd8]) {
        let mut pos = 2;
        let mut dims = None;
        let mut scan = false;
        let mut ended = false;
        while pos < b.len() {
            if b[pos] != 0xff {
                return Err(invalid("invalid JPEG marker"));
            }
            while pos < b.len() && b[pos] == 0xff {
                pos += 1;
            }
            if pos >= b.len() {
                return Err(invalid("truncated JPEG marker"));
            }
            let marker = b[pos];
            pos += 1;
            if marker == 0xd9 {
                ended = pos == b.len();
                break;
            }
            if marker == 0x00 || marker == 0xd8 {
                return Err(invalid("unexpected JPEG marker"));
            }
            if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                continue;
            }
            if b.len() - pos < 2 {
                return Err(invalid("truncated JPEG segment"));
            }
            let len = u16::from_be_bytes([b[pos], b[pos + 1]]) as usize;
            let end = pos
                .checked_add(len)
                .filter(|p| len >= 2 && *p <= b.len())
                .ok_or_else(|| invalid("invalid JPEG segment length"))?;
            if matches!(marker,0xc0..=0xc3|0xc5..=0xc7|0xc9..=0xcb|0xcd..=0xcf) {
                if len < 8 {
                    return Err(invalid("truncated JPEG frame"));
                }
                let h = u16::from_be_bytes([b[pos + 3], b[pos + 4]]) as u32;
                let w = u16::from_be_bytes([b[pos + 5], b[pos + 6]]) as u32;
                if dims.replace((w, h)).is_some() {
                    return Err(invalid("multiple JPEG frames unsupported"));
                }
            }
            pos = end;
            if marker == 0xda {
                scan = true;
                while pos < b.len() {
                    if b[pos] == 0xff {
                        if pos + 1 >= b.len() {
                            return Err(invalid("truncated JPEG scan"));
                        }
                        if b[pos + 1] == 0x00 || (0xd0..=0xd7).contains(&b[pos + 1]) {
                            pos += 2;
                            continue;
                        }
                        break;
                    }
                    pos += 1;
                }
            }
        }
        if !scan || !ended {
            return Err(invalid("JPEG scan or end missing"));
        }
        let (w, h) = dims.ok_or_else(|| invalid("JPEG dimensions missing"))?;
        (o::ImageOutputFormat::Jpeg, w, h)
    } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        if u32le(&b[4..8]) as u64 + 8 != b.len() as u64 {
            return Err(invalid("WebP length mismatch"));
        }
        let mut pos = 12;
        let mut dims = None;
        while pos < b.len() {
            if b.len() - pos < 8 {
                return Err(invalid("truncated WebP chunk"));
            }
            let size = u32le(&b[pos + 4..pos + 8]) as usize;
            let end = pos
                .checked_add(8)
                .and_then(|p| p.checked_add(size))
                .filter(|p| *p <= b.len())
                .ok_or_else(|| invalid("truncated WebP payload"))?;
            let data = &b[pos + 8..end];
            let kind = &b[pos..pos + 4];
            let next = if kind == b"VP8 " {
                if data.len() < 10 || data[0] & 1 != 0 || &data[3..6] != b"\x9d\x01\x2a" {
                    return Err(invalid("invalid WebP VP8 frame"));
                }
                Some((
                    u32::from(u16::from_le_bytes([data[6], data[7]]) & 0x3fff),
                    u32::from(u16::from_le_bytes([data[8], data[9]]) & 0x3fff),
                ))
            } else if kind == b"VP8L" {
                if data.len() < 5 || data[0] != 0x2f {
                    return Err(invalid("invalid WebP lossless frame"));
                }
                let bits = u32le(&data[1..5]);
                Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
            } else if kind == b"ANIM" || kind == b"ANMF" {
                return Err(invalid("animated WebP is not a generated still image"));
            } else {
                None
            };
            if let Some(d) = next
                && dims.replace(d).is_some()
            {
                return Err(invalid("multiple WebP images"));
            }
            pos = end
                .checked_add(size % 2)
                .filter(|p| *p <= b.len())
                .ok_or_else(|| invalid("missing WebP padding"))?;
        }
        let (w, h) = dims.ok_or_else(|| invalid("WebP image data missing"))?;
        (o::ImageOutputFormat::Webp, w, h)
    } else {
        return Err(invalid("unsupported or malformed PNG/JPEG/WebP image"));
    };
    if width == 0 || height == 0 {
        return Err(invalid("zero image dimensions"));
    }
    Ok(ImageMetadata {
        format,
        width,
        height,
    })
}

pub fn decode_image(
    encoded: &str,
    mime: Option<&str>,
    max_bytes: u64,
) -> Result<GeneratedImagePart, TransformError> {
    if !encoded.len().is_multiple_of(4) {
        return Err(invalid("invalid padded image base64 length"));
    }
    let padding = if encoded.ends_with("==") {
        2
    } else if encoded.ends_with('=') {
        1
    } else {
        0
    };
    let decoded_bound = (encoded.len() as u64 / 4)
        .saturating_mul(3)
        .saturating_sub(padding);
    if decoded_bound > max_bytes {
        return Err(TransformError::new(
            TransformErrorKind::Limit,
            "image.bytes",
            "decoded image exceeds cap",
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| invalid("invalid image base64"))?;
    if bytes.len() as u64 > max_bytes {
        return Err(TransformError::new(
            TransformErrorKind::Limit,
            "image.bytes",
            "decoded image exceeds cap",
        ));
    }
    let metadata = inspect_image(&bytes)?;
    if mime.is_some_and(|m| m != metadata.mime()) {
        return Err(invalid("declared MIME contradicts image bytes"));
    }
    Ok(GeneratedImagePart {
        bytes: bytes.into(),
        metadata,
        revised_prompt: None,
    })
}

pub fn image_response_from_responses(
    body: &r::response::GenerateContentResponseBody,
    _context: &ImageRequestContext,
    max_bytes: u64,
) -> Result<ImageCallResult, TransformError> {
    if body.id.trim().is_empty()
        || body.model.trim().is_empty()
        || body.created_at < 0
        || body.status != Some(r::response::ResponseStatus::Completed)
        || body.error.is_some()
        || body.incomplete_details.is_some()
    {
        return Err(invalid(
            "Responses generation did not complete successfully",
        ));
    }
    let mut out = None;
    let mut report = Report::default();
    for item in &body.output {
        match item {
            r::response::ResponseOutputItem::ImageGenerationCall(call) => {
                if call.status != r::input::ImageGenerationStatus::Completed
                    || call.id.trim().is_empty()
                {
                    return Err(invalid("image tool did not complete"));
                }
                let mut image = decode_image(
                    call.result
                        .as_deref()
                        .ok_or_else(|| invalid("missing image result"))?,
                    None,
                    max_bytes,
                )?;

                image.revised_prompt = call.revised_prompt.clone().flatten();
                if out.replace(image).is_some() {
                    return Err(invalid("expected exactly one image per call"));
                }
            }
            r::response::ResponseOutputItem::Reasoning(_) => {
                report.omitted("output.reasoning", "Images has no reasoning representation");
            }
            r::response::ResponseOutputItem::Message(message) => {
                if message.status != r::input::OutputMessageStatus::Completed
                    || message
                        .content
                        .iter()
                        .any(|p| matches!(p, r::input::OutputContent::Refusal(_)))
                {
                    return Err(invalid("incomplete or refused image response message"));
                }
                report.omitted(
                    "output.message",
                    "ancillary text has no Images representation and is not a revised prompt",
                );
            }
            _ => return Err(invalid("unexpected non-image tool output")),
        }
    }
    let usage = body
        .usage
        .as_ref()
        .and_then(Option::as_ref)
        .map(|u| {
            if [
                u.input_tokens,
                u.output_tokens,
                u.total_tokens,
                u.input_tokens_details.cache_write_tokens,
                u.input_tokens_details.cached_tokens,
                u.output_tokens_details.reasoning_tokens,
            ]
            .iter()
            .any(|n| *n < 0)
                || u.input_tokens.checked_add(u.output_tokens) != Some(u.total_tokens)
                || u.input_tokens_details
                    .cache_write_tokens
                    .checked_add(u.input_tokens_details.cached_tokens)
                    .is_none_or(|v| v > u.input_tokens)
                || u.output_tokens_details.reasoning_tokens > u.output_tokens
            {
                return Err(invalid("invalid Responses usage"));
            }
            Ok(ImageUsageFacts::Responses(u.clone().into_declared()))
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    Ok(ImageCallResult {
        image: out.ok_or_else(|| invalid("missing image result"))?,
        usage,
        response_id: Some(body.id.clone()),
        model: Some(body.model.clone()),
        report,
    })
}

pub fn image_response_from_gemini(
    body: &g::GenerateContentResponseBody,
    _context: &ImageRequestContext,
    max_bytes: u64,
) -> Result<ImageCallResult, TransformError> {
    if body
        .prompt_feedback
        .as_ref()
        .and_then(|p| p.block_reason)
        .is_some_and(|v| v != g::BlockReason::Unspecified)
    {
        return Err(invalid("Gemini blocked image generation"));
    }
    let candidates = body.candidates.as_deref().unwrap_or_default();
    if candidates.len() != 1 {
        return Err(invalid("expected exactly one Gemini candidate"));
    }
    let candidate = &candidates[0];
    if candidate.finish_reason != Some(g::FinishReason::Stop)
        || candidate.index.is_some_and(|i| i != 0)
    {
        return Err(invalid("Gemini candidate did not finish normally"));
    }
    let content = candidate
        .content
        .as_ref()
        .ok_or_else(|| invalid("missing Gemini image content"))?;
    if content.role.as_deref().is_some_and(|r| r != "model") {
        return Err(invalid("invalid Gemini candidate role"));
    }
    let mut out = None;
    let mut report = Report::default();
    for part in content.parts.as_deref().unwrap_or_default() {
        if part.function_call.is_some()
            || part.function_response.is_some()
            || part.file_data.is_some()
            || part.executable_code.is_some()
            || part.code_execution_result.is_some()
            || part.tool_call.is_some()
            || part.tool_response.is_some()
            || part.video_metadata.is_some()
        {
            return Err(invalid("unexpected non-image Gemini tool or media output"));
        }
        if part.text.is_some() && part.inline_data.is_some() {
            return Err(invalid("Gemini part contains multiple payloads"));
        }
        if part.thought == Some(true) || part.text.is_some() {
            report.omitted(
                "candidate.text/thought",
                "ancillary text has no Images representation and is not a revised prompt",
            );
            continue;
        }
        let data = part
            .inline_data
            .as_ref()
            .ok_or_else(|| invalid("unexpected empty Gemini output"))?;
        let image = decode_image(&data.data, Some(&data.mime_type), max_bytes)?;

        if out.replace(image).is_some() {
            return Err(invalid("expected exactly one Gemini image"));
        }
    }
    let usage = body
        .usage_metadata
        .as_ref()
        .map(|u| {
            if [
                u.prompt_token_count,
                u.candidates_token_count,
                u.cached_content_token_count,
                u.tool_use_prompt_token_count,
                u.thoughts_token_count,
                u.total_token_count,
            ]
            .into_iter()
            .flatten()
            .any(|n| n < 0)
            {
                return Err(invalid("negative Gemini usage"));
            }
            for details in [
                &u.prompt_tokens_details,
                &u.cache_tokens_details,
                &u.candidates_tokens_details,
                &u.tool_use_prompt_tokens_details,
            ]
            .into_iter()
            .flatten()
            {
                if details.iter().any(|d| d.token_count.is_some_and(|n| n < 0)) {
                    return Err(invalid("negative Gemini modality token count"));
                }
            }
            Ok(ImageUsageFacts::Gemini(u.clone().into_declared()))
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    Ok(ImageCallResult {
        image: out.ok_or_else(|| invalid("missing Gemini image"))?,
        usage,
        response_id: body.response_id.clone(),
        model: body.model_version.clone(),
        report,
    })
}
