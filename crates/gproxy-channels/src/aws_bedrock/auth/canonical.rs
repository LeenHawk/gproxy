use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_channel_api::ChannelError;

pub(super) fn headers(request: &http::Request<Bytes>) -> Result<(String, String), ChannelError> {
    let authority = request
        .uri()
        .authority()
        .ok_or_else(|| ChannelError::Prepare("AWS endpoint has no authority".into()))?;
    let mut headers = BTreeMap::<String, Vec<String>>::new();
    headers.insert("host".into(), vec![authority.as_str().into()]);
    for (name, value) in request.headers() {
        if matches!(
            name.as_str(),
            "authorization" | "connection" | "content-length" | "transfer-encoding" | "user-agent"
        ) {
            continue;
        }
        let value = value
            .to_str()
            .map_err(|_| ChannelError::Prepare("AWS signed header is not text".into()))?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        headers.entry(name.as_str().into()).or_default().push(value);
    }
    let signed = headers.keys().cloned().collect::<Vec<_>>().join(";");
    let canonical = headers
        .into_iter()
        .map(|(name, values)| format!("{name}:{}\n", values.join(",")))
        .collect();
    Ok((canonical, signed))
}

pub(super) fn uri(path: &str) -> Result<String, ChannelError> {
    if path.is_empty() {
        return Ok("/".into());
    }
    path.split('/')
        .map(|segment| percent_decode(segment).map(|bytes| encode(&bytes)))
        .collect::<Result<Vec<_>, _>>()
        .map(|segments| segments.join("/"))
}

pub(super) fn query(query: &str) -> Result<String, ChannelError> {
    let mut pairs = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            Ok((
                encode(&percent_decode(name)?),
                encode(&percent_decode(value)?),
            ))
        })
        .collect::<Result<Vec<_>, ChannelError>>()?;
    pairs.sort();
    Ok(pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&"))
}

fn percent_decode(value: &str) -> Result<Vec<u8>, ChannelError> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| ChannelError::Prepare("invalid percent encoding".into()))?;
            let text = std::str::from_utf8(pair)
                .map_err(|_| ChannelError::Prepare("invalid percent encoding".into()))?;
            output.push(
                u8::from_str_radix(text, 16)
                    .map_err(|_| ChannelError::Prepare("invalid percent encoding".into()))?,
            );
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    Ok(output)
}

fn encode(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for byte in value {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(*byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(*byte >> 4)]));
            output.push(char::from(HEX[usize::from(*byte & 0x0f)]));
        }
    }
    output
}
