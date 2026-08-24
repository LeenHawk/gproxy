use bytes::Bytes;
use gproxy_channel_api::ChannelError;

pub(super) struct SseFrame {
    pub(super) event: Option<String>,
    pub(super) data: String,
}

pub(super) fn parse(raw: &[u8]) -> Result<Option<SseFrame>, ChannelError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| ChannelError::Decode("Codex SSE frame is not UTF-8".into()))?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    Ok((!data.is_empty()).then(|| SseFrame {
        event,
        data: data.join("\n"),
    }))
}

pub(super) fn encode(event: Option<&str>, data: &str) -> Bytes {
    let mut output = String::new();
    if let Some(event) = event {
        output.push_str("event: ");
        output.push_str(event);
        output.push('\n');
    }
    for line in data.lines() {
        output.push_str("data: ");
        output.push_str(line);
        output.push('\n');
    }
    output.push('\n');
    Bytes::from(output)
}

pub(super) fn delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find(buffer, b"\n\n").map(|index| (index, 2));
    let crlf = find(buffer, b"\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (left, right) => left.or(right),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
