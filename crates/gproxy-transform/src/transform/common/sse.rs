/// A provider-neutral SSE transport frame.
///
/// This is framing, not a model event IR. Pair modules still own event payload
/// conversion after JSON decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    pub event: Option<String>,
    pub data: String,
}

impl SseFrame {
    pub fn data(data: impl Into<String>) -> Self {
        Self {
            event: None,
            data: data.into(),
        }
    }

    pub fn event(event: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            event: Some(event.into()),
            data: data.into(),
        }
    }

    pub fn encode(&self) -> String {
        let mut encoded = String::new();
        if let Some(event) = &self.event {
            encoded.push_str("event: ");
            encoded.push_str(event);
            encoded.push('\n');
        }
        for line in self.data.lines() {
            encoded.push_str("data: ");
            encoded.push_str(line);
            encoded.push('\n');
        }
        encoded.push('\n');
        encoded
    }
}

/// Incremental SSE frame decoder: feed raw body chunks, drain complete frames.
/// Tolerates CRLF, multi-line `data:`, and frames split across chunk
/// boundaries — including chunks split inside a multi-byte UTF-8 character.
/// Comments, `id:` and `retry:` lines are framing noise (dropped).
#[derive(Debug, Default)]
pub struct SseDecoder {
    buf: String,
    utf8: super::utf8::Utf8StreamDecoder,
    limits: SseLimits,
}

/// Bounds for untrusted SSE input. Both limits include framing bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SseLimits {
    /// Maximum bytes allowed in one blank-line-delimited SSE frame.
    pub max_frame_bytes: usize,
    /// Maximum bytes retained while waiting for complete SSE frames.
    pub max_buffer_bytes: usize,
}

impl Default for SseLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: 100 * 1024 * 1024,
            max_buffer_bytes: 100 * 1024 * 1024,
        }
    }
}

impl SseDecoder {
    /// Create a decoder with the default untrusted-input limits.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a decoder with explicit frame and buffer limits.
    pub fn with_limits(limits: SseLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// Append a chunk and return all complete (blank-line-terminated) frames.
    /// SSE is text by definition; genuinely invalid UTF-8 is replaced lossily,
    /// while an incomplete trailing sequence waits for the next chunk.
    ///
    /// Frames are parsed by BORROWING slices of the buffer behind an advancing
    /// cursor; consumed bytes are drained once at the end. (The former
    /// per-frame `drain(..).collect::<String>()` walked the frame char by char
    /// AND memmoved the buffer tail per frame — the top CPU hotspot of every
    /// streaming profile.)
    pub fn push(
        &mut self,
        chunk: &[u8],
    ) -> Result<Vec<SseFrame>, crate::transform::TransformError> {
        self.push_inner(chunk)
    }

    fn push_inner(
        &mut self,
        chunk: &[u8],
    ) -> Result<Vec<SseFrame>, crate::transform::TransformError> {
        let pending = self.buf.len().saturating_add(chunk.len());
        self.check_limit("buffer", self.limits.max_buffer_bytes, pending)?;
        self.utf8.decode_into(chunk, &mut self.buf);
        self.check_limit("buffer", self.limits.max_buffer_bytes, self.buf.len())?;
        if self.buf.contains('\r') {
            self.buf = self.buf.replace("\r\n", "\n");
        }
        let mut frames = Vec::new();
        let mut cursor = 0;
        while let Some(pos) = self.buf[cursor..].find("\n\n") {
            let end = cursor + pos + 2;
            self.check_limit("frame", self.limits.max_frame_bytes, end - cursor)?;
            if let Some(frame) = parse_frame(&self.buf[cursor..end]) {
                frames.push(frame);
            }
            cursor = end;
        }
        if cursor > 0 {
            self.buf.drain(..cursor);
        }
        self.check_limit("frame", self.limits.max_frame_bytes, self.buf.len())?;
        Ok(frames)
    }

    /// Drain a trailing, unterminated frame at end of stream (some upstreams
    /// omit the final blank line).
    pub fn finish(&mut self) -> Result<Option<SseFrame>, crate::transform::TransformError> {
        self.finish_inner()
    }

    fn finish_inner(&mut self) -> Result<Option<SseFrame>, crate::transform::TransformError> {
        self.utf8.flush(&mut self.buf);
        self.check_limit("frame", self.limits.max_frame_bytes, self.buf.len())?;
        let raw = std::mem::take(&mut self.buf);
        Ok(parse_frame(&raw))
    }

    fn check_limit(
        &self,
        limit: &'static str,
        max_bytes: usize,
        actual_bytes: usize,
    ) -> Result<(), crate::transform::TransformError> {
        if actual_bytes <= max_bytes {
            Ok(())
        } else {
            Err(crate::transform::TransformError::StreamLimitExceeded {
                limit,
                max_bytes,
                actual_bytes,
            })
        }
    }
}

fn parse_frame(raw: &str) -> Option<SseFrame> {
    let mut event = None;
    let mut data_lines: Vec<&str> = Vec::new();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event = Some(rest.trim_start().to_owned());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    Some(SseFrame {
        event,
        data: data_lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_split_across_chunks() {
        let mut d = SseDecoder::new();
        assert!(d.push(b"event: ping\nda").unwrap().is_empty());
        let frames = d.push(b"ta: {\"a\":1}\n\n: comment\ndata: x").unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event.as_deref(), Some("ping"));
        assert_eq!(frames[0].data, "{\"a\":1}");
        // trailing unterminated frame surfaces on finish()
        assert_eq!(d.finish().unwrap().unwrap().data, "x");
    }

    #[test]
    fn crlf_and_multiline_data() {
        let mut d = SseDecoder::new();
        let frames = d.push(b"data: l1\r\ndata: l2\r\n\r\n").unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "l1\nl2");
    }

    #[test]
    fn multibyte_char_split_across_chunks() {
        // "data: 汉字\n\n" split inside the 3-byte "汉" — must not yield U+FFFD.
        let mut d = SseDecoder::new();
        let bytes = "data: 汉字\n\n".as_bytes();
        assert!(d.push(&bytes[..7]).unwrap().is_empty()); // cuts "汉" after 1 byte
        let frames = d.push(&bytes[7..]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "汉字");
    }

    #[test]
    fn rejects_oversized_frame_and_buffer() {
        let limits = SseLimits {
            max_frame_bytes: 16,
            max_buffer_bytes: 32,
        };
        let mut d = SseDecoder::with_limits(limits);
        assert!(d.push(b"data: 12345678901\n\n").is_err());

        let mut d = SseDecoder::with_limits(limits);
        assert!(d.push(&[b'x'; 33]).is_err());
    }
}
