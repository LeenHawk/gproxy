//! Input parsing + file upload for `/v1/images/edits`.
//!
//! Clients send image-edit requests in two shapes:
//!
//! * **multipart/form-data** (OpenAI SDK default): parts are
//!   `image` (binary file), `prompt`, plus optional `mask`, `n`, `size`, etc.
//! * **application/json**: `images[].image_url` carries either a
//!   `data:<mime>;base64,<...>` URL or an HTTPS URL, plus `prompt`.
//!
//! We flatten both into a single [`ParsedEdit`] and upload the image
//! bytes to chatgpt.com via the 3-step files API before building a
//! `/f/conversation` body with an `image_asset_pointer`.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use wreq::Client;

use super::session::standard_headers;
use crate::response::UpstreamError;

const UPLOAD_BASE: &str = "https://chatgpt.com";

/// Parsed, transport-neutral representation of one image-edit request.
#[derive(Debug, Clone)]
pub struct ParsedEdit {
    /// Raw image bytes.
    pub image_bytes: Vec<u8>,
    /// Filename to announce to the upload API (must have an extension).
    pub filename: String,
    /// MIME type (`image/png`, `image/jpeg`, ...).
    pub mime_type: String,
    /// The edit prompt text.
    pub prompt: String,
}

/// Parse an incoming `/v1/images/edits` request body.
///
/// Autodetects multipart vs JSON from the body layout:
/// - Starts with `--<boundary>\r\n` → multipart.
/// - Otherwise assumed JSON.
pub fn parse_edit_body(body: &[u8]) -> Result<ParsedEdit, String> {
    if is_multipart(body) {
        parse_multipart(body)
    } else {
        parse_json(body)
    }
}

fn is_multipart(body: &[u8]) -> bool {
    body.starts_with(b"--") && body.iter().take(256).any(|b| *b == b'\r' || *b == b'\n')
}

fn parse_multipart(body: &[u8]) -> Result<ParsedEdit, String> {
    // Boundary = first line (minus leading `--`, trimmed).
    let newline = body
        .iter()
        .position(|b| *b == b'\n')
        .ok_or_else(|| "multipart: missing first newline".to_string())?;
    let first_line = &body[..newline];
    let first_line = first_line.strip_suffix(b"\r").unwrap_or(first_line);
    let boundary = first_line
        .strip_prefix(b"--")
        .ok_or_else(|| "multipart: first line does not start with --".to_string())?;
    if boundary.is_empty() {
        return Err("multipart: empty boundary".into());
    }

    let separator = {
        let mut sep = Vec::with_capacity(boundary.len() + 4);
        sep.extend_from_slice(b"\r\n--");
        sep.extend_from_slice(boundary);
        sep
    };

    // Skip the first boundary.
    let mut rest = &body[newline + 1..];

    let mut image_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut prompt: Option<String> = None;

    loop {
        // Each part starts here until the next boundary.
        let end = memmem(rest, &separator)
            .ok_or_else(|| "multipart: trailing boundary not found".to_string())?;
        let part = &rest[..end];
        let header_end = memmem(part, b"\r\n\r\n")
            .ok_or_else(|| "multipart: part header/body separator missing".to_string())?;
        let raw_headers = &part[..header_end];
        let part_body = &part[header_end + 4..];

        let (name, file_name, content_type) = parse_part_headers(raw_headers);
        match name.as_deref() {
            Some("image") | Some("image[]") | Some("image[0]") => {
                image_bytes = Some(part_body.to_vec());
                if let Some(f) = file_name {
                    filename = Some(f);
                }
                if let Some(ct) = content_type {
                    mime_type = Some(ct);
                }
            }
            Some("prompt") => {
                prompt = Some(String::from_utf8_lossy(part_body).into_owned());
            }
            _ => {
                // Ignore n, size, model, mask, etc. for now.
            }
        }

        // Advance past separator.
        let after = &rest[end + separator.len()..];
        // Next bytes are either `\r\n` (another part) or `--\r\n` (end).
        if after.starts_with(b"--") {
            break;
        }
        if let Some(stripped) = after.strip_prefix(b"\r\n") {
            rest = stripped;
        } else {
            rest = after;
        }
    }

    let image_bytes = image_bytes.ok_or_else(|| "multipart: missing image part".to_string())?;
    let prompt = prompt.unwrap_or_default();
    let filename = filename.unwrap_or_else(|| "image.png".to_string());
    let mime_type = mime_type
        .or_else(|| Some(guess_mime_from_name(&filename).to_string()))
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok(ParsedEdit {
        image_bytes,
        filename,
        mime_type,
        prompt,
    })
}

fn parse_part_headers(raw: &[u8]) -> (Option<String>, Option<String>, Option<String>) {
    let mut name: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut content_type: Option<String> = None;
    for line in raw.split(|b| *b == b'\n') {
        let line = std::str::from_utf8(line)
            .unwrap_or("")
            .trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line
            .to_ascii_lowercase()
            .strip_prefix("content-disposition:")
        {
            // Re-parse raw line case-sensitively for the filename value,
            // but lowercased key lookups below.
            let raw_rest = &line[line.len() - rest.len()..];
            for token in raw_rest.split(';') {
                let token = token.trim();
                if let Some(v) = token.strip_prefix("name=") {
                    name = Some(trim_quotes(v).to_string());
                } else if let Some(v) = token.strip_prefix("filename=") {
                    file_name = Some(trim_quotes(v).to_string());
                }
            }
        } else if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-type:") {
            content_type = Some(rest.trim().to_string());
        }
    }
    (name, file_name, content_type)
}

fn trim_quotes(s: &str) -> &str {
    s.trim().trim_start_matches('"').trim_end_matches('"')
}

fn memmem(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == *needle)
}

fn parse_json(body: &[u8]) -> Result<ParsedEdit, String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("edit body: {e}"))?;
    let prompt = v
        .get("prompt")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let image_ref = v
        .get("image")
        .and_then(|i| i.as_str())
        .or_else(|| {
            // OpenAI protocol schema nests under `images[0].image_url`.
            v.get("images")
                .and_then(|imgs| imgs.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.get("image_url").and_then(|u| u.as_str()))
        })
        .or_else(|| v.get("image_url").and_then(|u| u.as_str()))
        .ok_or_else(|| "edit body: missing image (data URL or https URL)".to_string())?;

    if let Some(rest) = image_ref.strip_prefix("data:") {
        // data:<mime>;base64,<bytes>
        let comma = rest
            .find(',')
            .ok_or_else(|| "edit body: malformed data URL".to_string())?;
        let header = &rest[..comma];
        let payload = &rest[comma + 1..];
        let mime_type = header
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = STANDARD
            .decode(payload)
            .map_err(|e| format!("edit body: base64 decode: {e}"))?;
        let ext = mime_to_ext(&mime_type);
        Ok(ParsedEdit {
            image_bytes: bytes,
            filename: format!("image.{ext}"),
            mime_type,
            prompt,
        })
    } else if image_ref.starts_with("http://") || image_ref.starts_with("https://") {
        Err("edit body: remote image_url not yet supported".into())
    } else {
        Err("edit body: image_url must be data URL or https URL".into())
    }
}

fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "bin",
    }
}

fn guess_mime_from_name(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else {
        "application/octet-stream"
    }
}

/// Outcome of the 3-step upload: the server-assigned file id plus
/// image dimensions. We need the dimensions in the `/f/conversation`
/// body's `image_asset_pointer` part.
#[derive(Debug, Clone)]
pub struct UploadResult {
    pub file_id: String,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub filename: String,
    pub mime_type: String,
}

/// Three-step upload of a raw image to chatgpt.com's files API:
///
/// 1. `POST /backend-api/files` with `{file_name, file_size, use_case, ...}`
///    returns `{upload_url, file_id}` where `upload_url` is a presigned
///    Azure Blob URL.
/// 2. `PUT <upload_url>` with the raw bytes and
///    `x-ms-blob-type: BlockBlob`.
/// 3. `POST /backend-api/files/process_upload_stream` with `{file_id, ...}`
///    to activate the file so /f/conversation can reference it.
pub async fn upload_image_to_chatgpt(
    client: &Client,
    access_token: &str,
    parsed: &ParsedEdit,
) -> Result<UploadResult, UpstreamError> {
    let (width, height) = probe_png_dimensions(&parsed.image_bytes).unwrap_or((1024, 1024));
    let size_bytes = parsed.image_bytes.len() as u64;

    // Step 1: request upload URL
    let step1_body = serde_json::json!({
        "file_name": parsed.filename,
        "file_size": size_bytes,
        "use_case": "multimodal",
        "timezone_offset_min": -480,
        "reset_rate_limits": false,
        "store_in_library": true,
        "library_persistence_mode": "opportunistic",
    });
    let step1: Value = client
        .post(format!("{UPLOAD_BASE}/backend-api/files"))
        .headers(standard_headers(access_token).into())
        .json(&step1_body)
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("upload step1: {e}")))?
        .json()
        .await
        .map_err(|e| UpstreamError::Channel(format!("upload step1 decode: {e}")))?;

    let upload_url = step1
        .get("upload_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            UpstreamError::Channel(format!(
                "upload step1: missing upload_url in {}",
                serde_json::to_string(&step1).unwrap_or_default()
            ))
        })?
        .to_string();
    let file_id = step1
        .get("file_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| UpstreamError::Channel("upload step1: missing file_id".into()))?
        .to_string();

    // Step 2: PUT raw bytes to Azure Blob.
    let step2 = client
        .put(&upload_url)
        .header("content-type", parsed.mime_type.as_str())
        .header("x-ms-blob-type", "BlockBlob")
        .body(parsed.image_bytes.clone())
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("upload step2: {e}")))?;
    if !step2.status().is_success() {
        let status = step2.status();
        let text = step2
            .text()
            .await
            .unwrap_or_else(|_| "<body read failed>".into());
        return Err(UpstreamError::Channel(format!(
            "upload step2 {status}: {}",
            text.chars().take(200).collect::<String>()
        )));
    }

    // Step 3: activate.
    let step3_body = serde_json::json!({
        "file_id": file_id,
        "use_case": "multimodal",
        "index_for_retrieval": false,
        "file_name": parsed.filename,
        "library_persistence_mode": "opportunistic",
        "metadata": {"store_in_library": true},
    });
    let step3 = client
        .post(format!(
            "{UPLOAD_BASE}/backend-api/files/process_upload_stream"
        ))
        .headers(standard_headers(access_token).into())
        .json(&step3_body)
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("upload step3: {e}")))?;
    if !step3.status().is_success() {
        let status = step3.status();
        let text = step3
            .text()
            .await
            .unwrap_or_else(|_| "<body read failed>".into());
        return Err(UpstreamError::Channel(format!(
            "upload step3 {status}: {}",
            text.chars().take(200).collect::<String>()
        )));
    }

    Ok(UploadResult {
        file_id,
        size_bytes,
        width,
        height,
        filename: parsed.filename.clone(),
        mime_type: parsed.mime_type.clone(),
    })
}

/// Best-effort PNG/JPEG/GIF/WEBP dimension probe. Returns `None` if the
/// header doesn't match a known format. The upload API doesn't strictly
/// require exact dimensions (the server re-reads them), but sending
/// correct values in `image_asset_pointer` is closer to browser
/// behaviour.
fn probe_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // PNG: 8-byte signature then `IHDR` chunk (length=13 at offset 8..12).
    // Width at offset 16..20, height at 20..24, big-endian.
    if bytes.len() > 24 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
    // JPEG: SOF0/SOF2 marker (0xFFC0 / 0xFFC2) carries dimensions.
    if bytes.len() > 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2;
        while i + 8 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC
            {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            let segment_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            i += 2 + segment_len;
        }
    }
    // GIF: 6-byte signature, then width/height little-endian at 6..10.
    if bytes.len() > 10 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return Some((w, h));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_data_url() {
        let png_bytes = minimal_png();
        let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&png_bytes));
        let body = serde_json::json!({
            "image": data_url,
            "prompt": "make it blue",
            "n": 1,
        });
        let parsed = parse_edit_body(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(parsed.mime_type, "image/png");
        assert_eq!(parsed.prompt, "make it blue");
        assert_eq!(parsed.image_bytes, png_bytes);
    }

    #[test]
    fn parses_multipart() {
        let png = minimal_png();
        let boundary = "----WebKitFormBoundaryABC";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"image\"; filename=\"x.png\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
        body.extend_from_slice(&png);
        body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"prompt\"\r\n\r\n");
        body.extend_from_slice(b"add a red hat");
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let parsed = parse_edit_body(&body).unwrap();
        assert_eq!(parsed.mime_type, "image/png");
        assert_eq!(parsed.prompt, "add a red hat");
        assert_eq!(parsed.image_bytes, png);
        assert_eq!(parsed.filename, "x.png");
    }

    #[test]
    fn probes_png_dimensions() {
        let png = minimal_png();
        assert_eq!(probe_png_dimensions(&png), Some((1, 1)));
    }

    fn minimal_png() -> Vec<u8> {
        // 1x1 transparent PNG, stable known header for dimension probing.
        vec![
            0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n', // signature
            0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', // IHDR length + type
            0x00, 0x00, 0x00, 0x01, // width = 1
            0x00, 0x00, 0x00, 0x01, // height = 1
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, // bit depth + CRC
        ]
    }
}
