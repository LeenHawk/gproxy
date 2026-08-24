use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) struct Event {
    pub raw: Bytes,
    pub value: Option<Value>,
}

#[derive(Default)]
pub(super) struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<Event>, ChannelError> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "ClaudeWeb SSE frame exceeds 16 MiB".into(),
            ));
        }
        let mut events = Vec::new();
        while let Some((end, delimiter)) = boundary(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            events.push(event(Bytes::from(raw))?);
        }
        Ok(events)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<Event>, ChannelError> {
        if self.buffer.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![event(Bytes::from(std::mem::take(&mut self.buffer)))?])
        }
    }

    pub(super) fn take_pending(&mut self) -> Option<Bytes> {
        (!self.buffer.is_empty()).then(|| Bytes::from(std::mem::take(&mut self.buffer)))
    }
}

pub(super) fn encode(value: &Value) -> Bytes {
    let event = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    Bytes::from(format!("event: {event}\ndata: {value}\n\n"))
}

fn event(raw: Bytes) -> Result<Event, ChannelError> {
    let text = std::str::from_utf8(&raw)
        .map_err(|_| ChannelError::Decode("ClaudeWeb SSE is not UTF-8".into()))?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>();
    let value = if data.is_empty() {
        None
    } else {
        Some(
            serde_json::from_str(&data.join("\n"))
                .map_err(|error| ChannelError::Decode(format!("ClaudeWeb SSE JSON: {error}")))?,
        )
    };
    Ok(Event { raw, value })
}

fn boundary(buffer: &[u8]) -> Option<(usize, usize)> {
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
        .position(|value| value == needle)
}
