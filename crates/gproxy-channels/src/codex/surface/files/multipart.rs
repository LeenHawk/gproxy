use bytes::Bytes;
use gproxy_channel_api::ChannelError;

pub(super) struct Upload {
    pub filename: String,
    pub purpose: String,
    pub mime_type: String,
    pub file: Bytes,
}

pub(super) fn parse(headers: &http::HeaderMap, body: &Bytes) -> Result<Upload, ChannelError> {
    let content_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ChannelError::Prepare("multipart content type missing".into()))?;
    let boundary = content_type
        .split(';')
        .find_map(|part| part.trim().strip_prefix("boundary="))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare("multipart boundary missing".into()))?;
    let mut filename = None;
    let mut purpose = None;
    let mut mime_type = "application/octet-stream".to_owned();
    let mut file = None;
    for part in super::super::super::multipart::split(body, boundary) {
        let part = part.strip_prefix(b"\r\n").unwrap_or(part);
        let Some(header_end) = find(part, b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&part[..header_end]);
        let data = trim_crlf(&part[header_end + 4..]);
        match attribute(&head, "name").as_deref() {
            Some("file") => {
                filename = attribute(&head, "filename");
                mime_type = head
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Type:"))
                    .map(str::trim)
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                file = Some(Bytes::copy_from_slice(data));
            }
            Some("purpose") => purpose = Some(String::from_utf8_lossy(data).into_owned()),
            _ => {}
        }
    }
    Ok(Upload {
        filename: filename
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ChannelError::Prepare("multipart filename missing".into()))?,
        purpose: purpose
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ChannelError::Prepare("multipart purpose missing".into()))?,
        mime_type,
        file: file.ok_or_else(|| ChannelError::Prepare("multipart file part missing".into()))?,
    })
}

fn trim_crlf(mut value: &[u8]) -> &[u8] {
    if value.ends_with(b"\r\n") {
        value = &value[..value.len() - 2];
    }
    value
}

fn attribute(headers: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    Some(headers.split(&marker).nth(1)?.split('"').next()?.to_owned())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_file_and_its_own_trailing_crlf_survive_parsing() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            "multipart/form-data; boundary=test".parse().unwrap(),
        );
        let body = Bytes::from_static(
            b"--test\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nassistants\r\n--test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n\x00\xff--test\r\n\r\n--test--\r\n",
        );
        let upload = parse(&headers, &body).expect("multipart upload");
        assert_eq!(upload.filename, "x.bin");
        assert_eq!(upload.purpose, "assistants");
        assert_eq!(upload.file.as_ref(), b"\x00\xff--test\r\n");
    }
}
