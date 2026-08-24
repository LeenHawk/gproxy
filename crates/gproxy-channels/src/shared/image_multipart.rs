use base64::Engine as _;
use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use http::HeaderMap;
use serde_json::{Map, Value};

pub(crate) fn object(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Map<String, Value>, ChannelError> {
    let Some(content_type) = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return json_object(body);
    };
    if !content_type
        .to_ascii_lowercase()
        .starts_with("multipart/form-data")
    {
        return json_object(body);
    }
    let boundary = content_type
        .split(';')
        .find_map(|part| part.trim().strip_prefix("boundary="))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare("multipart boundary missing".into()))?;
    parse(body, boundary)
}

fn json_object(body: &[u8]) -> Result<Map<String, Value>, ChannelError> {
    serde_json::from_slice::<Value>(body)
        .map_err(|error| ChannelError::Prepare(format!("image body is not JSON: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| ChannelError::Prepare("image body must be an object".into()))
}

fn parse(body: &[u8], boundary: &str) -> Result<Map<String, Value>, ChannelError> {
    let delimiter = format!("--{boundary}");
    let positions = find_all(body, delimiter.as_bytes());
    if positions.len() < 2 {
        return Err(ChannelError::Prepare("multipart body is malformed".into()));
    }
    let mut output = Map::new();
    for pair in positions.windows(2) {
        let start = pair[0] + delimiter.len();
        let mut part = &body[start..pair[1]];
        part = strip_framing(part);
        if part.is_empty() || part.starts_with(b"--") {
            continue;
        }
        let split = find(part, b"\r\n\r\n")
            .ok_or_else(|| ChannelError::Prepare("multipart part headers malformed".into()))?;
        let headers = std::str::from_utf8(&part[..split])
            .map_err(|_| ChannelError::Prepare("multipart part headers are not UTF-8".into()))?;
        let data = &part[split + 4..];
        append(&mut output, headers, data)?;
    }
    Ok(output)
}

fn append(output: &mut Map<String, Value>, headers: &str, data: &[u8]) -> Result<(), ChannelError> {
    let disposition = headers
        .lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with("content-disposition:")
        })
        .ok_or_else(|| ChannelError::Prepare("multipart content-disposition missing".into()))?;
    let name = quoted(disposition, "name=")
        .ok_or_else(|| ChannelError::Prepare("multipart field name missing".into()))?;
    let filename = quoted(disposition, "filename=");
    let value = if filename.is_some() {
        let mime = headers
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("application/octet-stream");
        Value::String(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(data)
        ))
    } else {
        let text = std::str::from_utf8(data)
            .map_err(|_| ChannelError::Prepare("multipart field is not UTF-8".into()))?;
        Value::String(text.into())
    };
    let name = name.strip_suffix("[]").unwrap_or(name);
    if name == "image" || name == "images" {
        push(output, "images", value);
    } else if output.contains_key(name) {
        push(output, name, value);
    } else {
        output.insert(name.into(), value);
    }
    Ok(())
}

pub(crate) fn json_fields(output: &mut Map<String, Value>, names: &[&str]) {
    for name in names {
        let Some(Value::String(text)) = output.get_mut(*name) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<Value>(text)
            && !value.is_string()
        {
            *output.get_mut(*name).expect("field remains present") = value;
        }
    }
}

fn push(output: &mut Map<String, Value>, name: &str, value: Value) {
    match output
        .entry(name)
        .or_insert_with(|| Value::Array(Vec::new()))
    {
        Value::Array(values) => values.push(value),
        existing => {
            let first = std::mem::replace(existing, Value::Array(Vec::new()));
            *existing = Value::Array(vec![first, value]);
        }
    }
}

fn quoted<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(key))?;
    rest.strip_prefix('"')?.strip_suffix('"')
}

fn strip_framing(mut value: &[u8]) -> &[u8] {
    if value.starts_with(b"\r\n") {
        value = &value[2..];
    }
    if value.ends_with(b"\r\n") {
        value = &value[..value.len() - 2];
    }
    value
}

fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(index, value)| (value == needle).then_some(index))
        .collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|value| value == needle)
}
