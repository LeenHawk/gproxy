use base64::Engine as _;
use bytes::Bytes;
use gproxy_protocol::{Operation, OperationKey, OperationKind, WireFamily};
use http::{HeaderMap, HeaderValue, header};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::error::CoreError;

pub(super) fn normalize(headers: &HeaderMap, body: Bytes) -> Result<(Bytes, bool), CoreError> {
    let Some(content_type) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
        })
    else {
        return Ok((body, false));
    };
    let boundary = boundary(content_type, &body)
        .ok_or_else(|| CoreError::Transform("multipart/form-data: missing boundary".into()))?;
    let delimiter = [b"--".as_slice(), boundary.as_slice()].concat();
    let mut cursor = find(&body, &delimiter)
        .ok_or_else(|| CoreError::Transform("multipart first boundary not found".into()))?
        + delimiter.len();
    let mut object = Map::new();
    loop {
        let rest = &body[cursor..];
        if rest.starts_with(b"--") {
            break;
        }
        let rest = strip_newline(rest);
        let end = find(rest, &delimiter)
            .ok_or_else(|| CoreError::Transform("multipart trailing boundary not found".into()))?;
        let raw = trim_newline(&rest[..end]);
        if !raw.is_empty() {
            let part = parse_part(raw)?;
            if let Some(name) = part.name.as_deref() {
                let (name, array) = form_name(name);
                insert(&mut object, name, part.value(), array);
            }
        }
        cursor = body.len() - rest.len() + end + delimiter.len();
    }
    Ok((
        Bytes::from(
            serde_json::to_vec(&object).map_err(|error| CoreError::Transform(error.to_string()))?,
        ),
        true,
    ))
}

pub(super) fn restore(
    key: OperationKey,
    headers: &mut HeaderMap,
    body: Bytes,
) -> Result<Bytes, CoreError> {
    if key.kind != OperationKind::Family(WireFamily::OpenAi)
        || !matches!(key.operation, Operation::EditImage | Operation::CreateVideo)
    {
        return Ok(body);
    }
    let fields: Map<String, Value> = serde_json::from_slice(&body)
        .map_err(|error| CoreError::Transform(format!("media target JSON: {error}")))?;
    let digest = Sha256::digest(&body);
    let boundary = format!(
        "gproxy-media-{}",
        digest[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let mut output = Vec::with_capacity(body.len());
    for (name, value) in fields {
        append_value(&mut output, &boundary, key.operation, &name, value)?;
    }
    output.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
            .map_err(|error| CoreError::Transform(error.to_string()))?,
    );
    headers.remove(header::CONTENT_LENGTH);
    Ok(Bytes::from(output))
}

struct Part<'a> {
    name: Option<String>,
    filename: Option<String>,
    content_type: Option<String>,
    body: &'a [u8],
}

impl Part<'_> {
    fn value(self) -> Value {
        if self.filename.is_some() || self.content_type.is_some() {
            let mime = self
                .content_type
                .unwrap_or_else(|| "application/octet-stream".into());
            return Value::String(format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(self.body)
            ));
        }
        Value::String(String::from_utf8_lossy(self.body).into_owned())
    }
}

fn parse_part(raw: &[u8]) -> Result<Part<'_>, CoreError> {
    let (headers, body) = split(raw, b"\r\n\r\n")
        .or_else(|| split(raw, b"\n\n"))
        .ok_or_else(|| CoreError::Transform("multipart part has no body separator".into()))?;
    let (mut name, mut filename, mut content_type) = (None, None, None);
    for line in headers.split(|byte| *byte == b'\n') {
        let line = String::from_utf8_lossy(line);
        let line = line.trim_end_matches('\r');
        let Some((header_name, value)) = line.split_once(':') else {
            continue;
        };
        if header_name.eq_ignore_ascii_case("content-disposition") {
            for parameter in value.split(';').skip(1) {
                let Some((key, value)) = parameter.trim().split_once('=') else {
                    continue;
                };
                let value = value.trim().trim_matches('"').to_owned();
                if key.eq_ignore_ascii_case("name") {
                    name = Some(value);
                } else if key.eq_ignore_ascii_case("filename") {
                    filename = Some(value);
                }
            }
        } else if header_name.eq_ignore_ascii_case("content-type") {
            content_type = Some(value.trim().to_owned());
        }
    }
    Ok(Part {
        name,
        filename,
        content_type,
        body,
    })
}

fn append_value(
    output: &mut Vec<u8>,
    boundary: &str,
    operation: Operation,
    name: &str,
    value: Value,
) -> Result<(), CoreError> {
    if let Value::Array(values) = value {
        for value in values {
            append_value(output, boundary, operation, &format!("{name}[]"), value)?;
        }
        return Ok(());
    }
    let base_name = name.strip_suffix("[]").unwrap_or(name);
    let file = matches!(operation, Operation::EditImage)
        && matches!(base_name, "image" | "images" | "mask")
        || operation == Operation::CreateVideo && base_name == "input_reference";
    output.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    let file_value = value.as_str().or_else(|| {
        value
            .get("image_url")
            .or_else(|| value.get("file_id"))
            .and_then(Value::as_str)
    });
    if file && let Some(file_value) = file_value {
        if !file_value.starts_with("data:") {
            return Err(CoreError::Transform(
                "multipart media target requires inline data; references are not downloaded".into(),
            ));
        }
        let (mime, data) = decode_data_url(file_value)?;
        output.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{name}\"; filename=\"media.bin\"\r\nContent-Type: {mime}\r\n\r\n"
            )
            .as_bytes(),
        );
        output.extend_from_slice(&data);
    } else {
        output.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        output.extend_from_slice(text.as_bytes());
    }
    output.extend_from_slice(b"\r\n");
    Ok(())
}

fn decode_data_url(value: &str) -> Result<(&str, Vec<u8>), CoreError> {
    let (metadata, payload) = value
        .strip_prefix("data:")
        .and_then(|value| value.split_once(','))
        .ok_or_else(|| CoreError::Transform("media file is not a data URL".into()))?;
    let mime = metadata
        .strip_suffix(";base64")
        .ok_or_else(|| CoreError::Transform("media data URL is not base64".into()))?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| CoreError::Transform(format!("media base64: {error}")))?;
    Ok((mime, data))
}

fn boundary(content_type: &str, body: &[u8]) -> Option<Vec<u8>> {
    content_type
        .split(';')
        .skip(1)
        .find_map(|parameter| {
            let (key, value) = parameter.trim().split_once('=')?;
            key.eq_ignore_ascii_case("boundary")
                .then(|| value.trim().trim_matches('"').as_bytes().to_vec())
        })
        .or_else(|| {
            body.split(|byte| *byte == b'\n')
                .next()?
                .strip_suffix(b"\r")
                .unwrap_or(body)
                .strip_prefix(b"--")
                .map(<[u8]>::to_vec)
        })
}

fn form_name(name: &str) -> (String, bool) {
    name.strip_suffix("[]")
        .map(|name| (name.to_owned(), true))
        .unwrap_or_else(|| (name.to_owned(), false))
}

fn insert(object: &mut Map<String, Value>, name: String, value: Value, array: bool) {
    match object.remove(&name) {
        Some(Value::Array(mut values)) => {
            values.push(value);
            object.insert(name, Value::Array(values));
        }
        Some(existing) => {
            object.insert(name, Value::Array(vec![existing, value]));
        }
        None if array => {
            object.insert(name, Value::Array(vec![value]));
        }
        None => {
            object.insert(name, value);
        }
    }
}

fn split<'a>(value: &'a [u8], delimiter: &[u8]) -> Option<(&'a [u8], &'a [u8])> {
    let index = find(value, delimiter)?;
    Some((&value[..index], &value[index + delimiter.len()..]))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn strip_newline(value: &[u8]) -> &[u8] {
    value
        .strip_prefix(b"\r\n")
        .or_else(|| value.strip_prefix(b"\n"))
        .unwrap_or(value)
}

fn trim_newline(value: &[u8]) -> &[u8] {
    value
        .strip_suffix(b"\r\n")
        .or_else(|| value.strip_suffix(b"\n"))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_images_normalize_and_restore_without_losing_binary_bytes() {
        let boundary = "gproxy-test-boundary";
        let body = Bytes::from(
            [
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\ndraw\r\n"
                )
                .into_bytes(),
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"image[]\"; filename=\"input.png\"\r\nContent-Type: image/png\r\n\r\n"
                )
                .into_bytes(),
                vec![0, 255, 10, 13],
                format!("\r\n--{boundary}--\r\n").into_bytes(),
            ]
            .concat(),
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}")).unwrap(),
        );
        let (json, normalized) = normalize(&headers, body).unwrap();
        assert!(normalized);
        let request: gproxy_protocol::openai::images::EditImageRequest =
            serde_json::from_slice(&json).unwrap();
        assert_eq!(request.images.len(), 1);
        assert!(
            request.images[0]
                .image_url
                .as_deref()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );

        let mut target_headers = HeaderMap::new();
        let target = OperationKey::family(Operation::EditImage, WireFamily::OpenAi);
        let restored = restore(
            target,
            &mut target_headers,
            Bytes::from(serde_json::to_vec(&request).unwrap()),
        )
        .unwrap();
        assert!(
            target_headers[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("multipart/form-data; boundary=gproxy-media-")
        );
        assert!(restored.windows(4).any(|window| window == [0, 255, 10, 13]));
    }
}
