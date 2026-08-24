use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use http::HeaderMap;

use super::Part;

pub(super) fn parts(headers: &HeaderMap, body: &Bytes) -> Result<Option<Vec<Part>>, ChannelError> {
    let Some(content_type) = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    if !content_type
        .to_ascii_lowercase()
        .starts_with("multipart/form-data")
    {
        return Ok(None);
    }
    let boundary = content_type
        .split(';')
        .find_map(|part| part.trim().strip_prefix("boundary="))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare("multipart boundary missing".into()))?;
    parse(body, boundary).map(Some)
}

fn parse(body: &[u8], boundary: &str) -> Result<Vec<Part>, ChannelError> {
    let delimiter = format!("--{boundary}");
    let positions = find_all(body, delimiter.as_bytes());
    if positions.len() < 2 {
        return Err(ChannelError::Prepare("multipart body is malformed".into()));
    }
    positions
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0] + delimiter.len();
            let part = strip_framing(&body[start..pair[1]]);
            (!part.is_empty() && !part.starts_with(b"--")).then_some(part)
        })
        .map(parse_part)
        .collect()
}

fn parse_part(part: &[u8]) -> Result<Part, ChannelError> {
    let split = find(part, b"\r\n\r\n")
        .ok_or_else(|| ChannelError::Prepare("multipart part headers malformed".into()))?;
    let headers = std::str::from_utf8(&part[..split])
        .map_err(|_| ChannelError::Prepare("multipart part headers are not UTF-8".into()))?;
    let disposition = headers
        .lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with("content-disposition:")
        })
        .ok_or_else(|| ChannelError::Prepare("multipart content-disposition missing".into()))?;
    let name = quoted(disposition, "name=")
        .ok_or_else(|| ChannelError::Prepare("multipart field name missing".into()))?;
    let file = quoted(disposition, "filename=").is_some();
    let mime = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    Ok(Part {
        name: name.into(),
        file,
        mime,
        data: part[split + 4..].to_vec(),
    })
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
