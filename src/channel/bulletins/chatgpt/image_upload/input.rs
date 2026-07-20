//! Image edit input parsing and image-header probing.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

#[derive(Debug, Clone)]
pub(in crate::channel::bulletins::chatgpt) struct ParsedEdit {
    pub image_bytes: Vec<u8>,
    pub filename: String,
    pub mime_type: String,
    pub prompt: String,
}

pub(in crate::channel::bulletins::chatgpt) fn parse_edit_body(
    body: &[u8],
) -> Result<ParsedEdit, String> {
    if is_multipart(body) {
        parse_multipart(body)
    } else {
        parse_json(body)
    }
}

fn is_multipart(body: &[u8]) -> bool {
    body.starts_with(b"--")
        && body
            .iter()
            .take(256)
            .any(|byte| matches!(byte, b'\r' | b'\n'))
}

fn parse_multipart(body: &[u8]) -> Result<ParsedEdit, String> {
    let newline = body
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("multipart: missing first newline")?;
    let first_line = body[..newline]
        .strip_suffix(b"\r")
        .unwrap_or(&body[..newline]);
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
    let mut image_bytes = None;
    let mut filename = None;
    let mut mime_type = None;
    let mut prompt = None;
    loop {
        let end = memmem(rest, &separator).ok_or("multipart: trailing boundary not found")?;
        let part = &rest[..end];
        let header_end =
            memmem(part, b"\r\n\r\n").ok_or("multipart: part header/body separator missing")?;
        let (name, part_filename, content_type) = parse_part_headers(&part[..header_end]);
        let part_body = &part[header_end + 4..];
        match name.as_deref() {
            Some("image") | Some("image[]") | Some("image[0]") => {
                image_bytes = Some(part_body.to_vec());
                filename = part_filename.or(filename);
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
    let (mut name, mut filename, mut content_type) = (None, None, None);
    for line in raw.split(|byte| *byte == b'\n') {
        let line = std::str::from_utf8(line)
            .unwrap_or("")
            .trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-disposition:") {
            let original = &line[line.len() - rest.len()..];
            for token in original.split(';').map(str::trim) {
                if let Some(value) = token.strip_prefix("name=") {
                    name = Some(trim_quotes(value).to_string());
                } else if let Some(value) = token.strip_prefix("filename=") {
                    filename = Some(trim_quotes(value).to_string());
                }
            }
        } else if let Some(rest) = lower.strip_prefix("content-type:") {
            content_type = Some(rest.trim().to_string());
        }
    }
    (name, filename, content_type)
}

fn trim_quotes(value: &str) -> &str {
    value.trim().trim_start_matches('"').trim_end_matches('"')
}

fn memmem(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&index| haystack[index..index + needle.len()] == *needle)
}

fn parse_json(body: &[u8]) -> Result<ParsedEdit, String> {
    let value: Value = serde_json::from_slice(body).map_err(|e| format!("edit body: {e}"))?;
    let prompt = value
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let image_ref = value
        .get("image")
        .and_then(image_ref_str)
        .or_else(|| {
            value
                .get("images")
                .and_then(Value::as_array)
                .and_then(|images| images.first())
                .and_then(image_ref_str)
        })
        .or_else(|| value.get("image_url").and_then(Value::as_str))
        .ok_or("edit body: missing image (data URL)")?;

    if let Some(rest) = image_ref.strip_prefix("data:") {
        let comma = rest.find(',').ok_or("edit body: malformed data URL")?;
        let mime_type = rest[..comma]
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
        let image_bytes = STANDARD
            .decode(&rest[comma + 1..])
            .map_err(|e| format!("edit body: base64 decode: {e}"))?;
        Ok(ParsedEdit {
            image_bytes,
            filename: format!("image.{}", mime_to_ext(&mime_type)),
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

pub(super) fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() > 24 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((width, height));
    }
    if bytes.len() > 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut index = 2;
        while index + 8 < bytes.len() {
            if bytes[index] != 0xFF {
                index += 1;
                continue;
            }
            let marker = bytes[index + 1];
            if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC
            {
                let height = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]) as u32;
                let width = u16::from_be_bytes([bytes[index + 7], bytes[index + 8]]) as u32;
                return Some((width, height));
            }
            let segment_len = u16::from_be_bytes([bytes[index + 2], bytes[index + 3]]) as usize;
            index += 2 + segment_len;
        }
    }
    if bytes.len() > 10 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return Some((width, height));
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
