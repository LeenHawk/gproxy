//! Input parsing + 3-step file upload for `/v1/images/edits`.
//!
//! Clients send image-edit requests as **multipart/form-data** (`image` +
//! `prompt` parts; OpenAI SDK default) or as **JSON** (`{image, prompt}` where
//! `image` is a `data:<mime>;base64,…` URL). Both flatten into [`ParsedEdit`],
//! whose bytes are uploaded to chatgpt.com via the 3-step files API before the
//! `/f/conversation` body references them via an `image_asset_pointer`.
//!
//! Ported from v1 `channels/chatgpt/image_edit.rs`, adapted from `wreq::Client`
//! to the v2 [`UpstreamClient`].

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use serde_json::Value;

use super::headers::standard_headers;
use crate::http::client::UpstreamClient;

/// Parsed, transport-neutral representation of one image-edit request.
#[derive(Debug, Clone)]
pub(super) struct ParsedEdit {
    pub image_bytes: Vec<u8>,
    pub filename: String,
    pub mime_type: String,
    pub prompt: String,
}

/// Parse an `/v1/images/edits` body, autodetecting multipart vs JSON.
pub(super) fn parse_edit_body(body: &[u8]) -> Result<ParsedEdit, String> {
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
    let newline = body
        .iter()
        .position(|b| *b == b'\n')
        .ok_or("multipart: missing first newline")?;
    let first_line = &body[..newline];
    let first_line = first_line.strip_suffix(b"\r").unwrap_or(first_line);
    let boundary = first_line
        .strip_prefix(b"--")
        .ok_or("multipart: first line does not start with --")?;
    if boundary.is_empty() {
        return Err("multipart: empty boundary".into());
    }
    let mut separator = Vec::with_capacity(boundary.len() + 4);
    separator.extend_from_slice(b"\r\n--");
    separator.extend_from_slice(boundary);

    let mut rest = &body[newline + 1..];
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut prompt: Option<String> = None;

    loop {
        let end = memmem(rest, &separator).ok_or("multipart: trailing boundary not found")?;
        let part = &rest[..end];
        let header_end =
            memmem(part, b"\r\n\r\n").ok_or("multipart: part header/body separator missing")?;
        let (name, file_name, content_type) = parse_part_headers(&part[..header_end]);
        let part_body = &part[header_end + 4..];
        match name.as_deref() {
            Some("image") | Some("image[]") | Some("image[0]") => {
                image_bytes = Some(part_body.to_vec());
                filename = file_name.or(filename);
                mime_type = content_type.or(mime_type);
            }
            Some("prompt") => prompt = Some(String::from_utf8_lossy(part_body).into_owned()),
            _ => {}
        }

        let after = &rest[end + separator.len()..];
        if after.starts_with(b"--") {
            break;
        }
        rest = after.strip_prefix(b"\r\n").unwrap_or(after);
    }

    let image_bytes = image_bytes.ok_or("multipart: missing image part")?;
    let filename = filename.unwrap_or_else(|| "image.png".to_string());
    let mime_type = mime_type.unwrap_or_else(|| guess_mime_from_name(&filename).to_string());
    Ok(ParsedEdit {
        image_bytes,
        filename,
        mime_type,
        prompt: prompt.unwrap_or_default(),
    })
}

fn parse_part_headers(raw: &[u8]) -> (Option<String>, Option<String>, Option<String>) {
    let (mut name, mut file_name, mut content_type) = (None, None, None);
    for line in raw.split(|b| *b == b'\n') {
        let line = std::str::from_utf8(line)
            .unwrap_or("")
            .trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-disposition:") {
            let raw_rest = &line[line.len() - rest.len()..];
            for token in raw_rest.split(';') {
                let token = token.trim();
                if let Some(v) = token.strip_prefix("name=") {
                    name = Some(trim_quotes(v).to_string());
                } else if let Some(v) = token.strip_prefix("filename=") {
                    file_name = Some(trim_quotes(v).to_string());
                }
            }
        } else if let Some(rest) = lower.strip_prefix("content-type:") {
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
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let image_ref = v
        .get("image")
        .and_then(image_ref_str)
        .or_else(|| {
            v.get("images")
                .and_then(|imgs| imgs.as_array())
                .and_then(|a| a.first())
                .and_then(image_ref_str)
        })
        .or_else(|| v.get("image_url").and_then(Value::as_str))
        .ok_or("edit body: missing image (data URL)")?;

    if let Some(rest) = image_ref.strip_prefix("data:") {
        let comma = rest.find(',').ok_or("edit body: malformed data URL")?;
        let mime_type = rest[..comma]
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = STANDARD
            .decode(&rest[comma + 1..])
            .map_err(|e| format!("edit body: base64 decode: {e}"))?;
        let ext = mime_to_ext(&mime_type);
        Ok(ParsedEdit {
            image_bytes: bytes,
            filename: format!("image.{ext}"),
            mime_type,
            prompt,
        })
    } else if image_ref.starts_with("http://") || image_ref.starts_with("https://") {
        Err("edit body: remote image_url not supported".into())
    } else {
        Err("edit body: image must be a data URL".into())
    }
}

fn image_ref_str(value: &Value) -> Option<&str> {
    match value {
        Value::String(value) => Some(value),
        Value::Array(values) => values.first().and_then(image_ref_str),
        Value::Object(object) => object.get("image_url").and_then(Value::as_str),
        _ => None,
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

/// Server-assigned file id + image dimensions, needed for the conversation
/// body's `image_asset_pointer`.
#[derive(Debug, Clone)]
pub(super) struct UploadResult {
    pub file_id: String,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub filename: String,
    pub mime_type: String,
}

/// Three-step raw-image upload to chatgpt.com's files API:
/// 1. `POST /backend-api/files` → `{upload_url, file_id}` (presigned Azure Blob).
/// 2. `PUT <upload_url>` raw bytes with `x-ms-blob-type: BlockBlob`.
/// 3. `POST /backend-api/files/process_upload_stream` to activate the file.
pub(super) async fn upload(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
    base: &str,
    parsed: &ParsedEdit,
) -> Result<UploadResult, String> {
    let token = secret
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("missing access_token")?;
    let (width, height) = probe_dimensions(&parsed.image_bytes).unwrap_or((1024, 1024));
    let size_bytes = parsed.image_bytes.len() as u64;

    // Step 1: request the upload URL.
    let step1_body = serde_json::json!({
        "file_name": parsed.filename,
        "file_size": size_bytes,
        "use_case": "multimodal",
        "timezone_offset_min": -480,
        "reset_rate_limits": false,
        "store_in_library": true,
        "library_persistence_mode": "opportunistic",
    });
    let step1 = post_json(
        client,
        token,
        &format!("{base}/backend-api/files"),
        &step1_body,
    )
    .await?;
    if !step1.status().is_success() {
        return Err(format!(
            "upload step1 {}: {}",
            step1.status(),
            snip(step1.body())
        ));
    }
    let s1: Value =
        serde_json::from_slice(step1.body()).map_err(|e| format!("upload step1 decode: {e}"))?;
    let upload_url = s1
        .get("upload_url")
        .and_then(Value::as_str)
        .ok_or("upload step1: missing upload_url")?
        .to_string();
    let file_id = s1
        .get("file_id")
        .and_then(Value::as_str)
        .ok_or("upload step1: missing file_id")?
        .to_string();

    // Step 2: PUT raw bytes to Azure Blob.
    let mut put_req = http::Request::put(&upload_url)
        .body(Bytes::from(parsed.image_bytes.clone()))
        .map_err(|e| format!("upload step2 build: {e}"))?;
    let h = put_req.headers_mut();
    if let Ok(v) = http::HeaderValue::from_str(&parsed.mime_type) {
        h.insert(http::header::CONTENT_TYPE, v);
    }
    h.insert(
        http::HeaderName::from_static("x-ms-blob-type"),
        http::HeaderValue::from_static("BlockBlob"),
    );
    let step2 = client
        .send(put_req)
        .await
        .map_err(|e| format!("upload step2: {e}"))?;
    if !step2.status().is_success() {
        return Err(format!(
            "upload step2 {}: {}",
            step2.status(),
            snip(step2.body())
        ));
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
    let step3 = post_json(
        client,
        token,
        &format!("{base}/backend-api/files/process_upload_stream"),
        &step3_body,
    )
    .await?;
    if !step3.status().is_success() {
        return Err(format!(
            "upload step3 {}: {}",
            step3.status(),
            snip(step3.body())
        ));
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

/// POST a JSON body with the standard chatgpt-web headers.
async fn post_json(
    client: &Arc<dyn UpstreamClient>,
    token: &str,
    url: &str,
    body: &Value,
) -> Result<http::Response<Bytes>, String> {
    let bytes = serde_json::to_vec(body).map_err(|e| format!("encode: {e}"))?;
    let mut req = http::Request::post(url)
        .body(Bytes::from(bytes))
        .map_err(|e| format!("build: {e}"))?;
    *req.headers_mut() = standard_headers(token);
    client.send(req).await.map_err(|e| format!("send: {e}"))
}

fn snip(body: &[u8]) -> String {
    String::from_utf8_lossy(body).chars().take(200).collect()
}

/// Attach an uploaded image onto the conversation body's single user message:
/// the content becomes `multimodal_text`, `parts[0]` an `image_asset_pointer`,
/// the prompt text `parts[1]`, and `metadata.attachments[0]` describes the file.
pub(super) fn attach_uploaded_image(
    body: &mut serde_json::Map<String, Value>,
    upload: &UploadResult,
) {
    let Some(messages) = body.get_mut("messages").and_then(|m| m.as_array_mut()) else {
        return;
    };
    if messages.is_empty() {
        return;
    }
    let user_msg = &mut messages[0];
    let prompt_text = user_msg
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let asset = serde_json::json!({
        "content_type": "image_asset_pointer",
        "asset_pointer": format!("sediment://{}", upload.file_id),
        "size_bytes": upload.size_bytes,
        "width": upload.width,
        "height": upload.height,
    });
    if let Some(obj) = user_msg.get_mut("content").and_then(Value::as_object_mut) {
        obj.insert(
            "content_type".into(),
            Value::String("multimodal_text".into()),
        );
        let mut parts = vec![asset];
        if !prompt_text.is_empty() {
            parts.push(Value::String(prompt_text));
        }
        obj.insert("parts".into(), Value::Array(parts));
    }
    if let Some(md) = user_msg.get_mut("metadata").and_then(Value::as_object_mut) {
        md.insert(
            "attachments".into(),
            Value::Array(vec![serde_json::json!({
                "id": upload.file_id,
                "size": upload.size_bytes,
                "name": upload.filename,
                "mime_type": upload.mime_type,
                "width": upload.width,
                "height": upload.height,
                "source": "library",
                "is_big_paste": false,
            })]),
        );
    }
}

/// Best-effort PNG/JPEG/GIF dimension probe (the server re-reads them; correct
/// values just match browser behaviour). `None` on an unrecognised header.
fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() > 24 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
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

    fn minimal_png() -> Vec<u8> {
        vec![
            0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n', 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89,
        ]
    }

    #[test]
    fn parses_json_data_url() {
        let png = minimal_png();
        let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&png));
        let body = serde_json::json!({"image": [data_url], "prompt": "make it blue", "n": 1});
        let parsed = parse_edit_body(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(parsed.mime_type, "image/png");
        assert_eq!(parsed.prompt, "make it blue");
        assert_eq!(parsed.image_bytes, png);
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
        assert_eq!(probe_dimensions(&minimal_png()), Some((1, 1)));
    }
}
