//! Patch error records on native CLI streams without changing keepalives,
//! unknown events, or the framing of other records.
use super::*;
use crate::channels::codex::retry;

#[derive(Default)]
struct Records {
    record: Vec<u8>,
    line_has_data: bool,
    after_cr: bool,
}

impl Records {
    fn push(&mut self, chunk: &[u8], fallback: &str) -> Bytes {
        let mut out = Vec::with_capacity(chunk.len());
        // CR, LF and CRLF may each straddle transport chunks.
        for part in chunk.split_inclusive(|b| matches!(b, b'\r' | b'\n')) {
            let crlf = self.after_cr && part == b"\n";
            self.after_cr = part.ends_with(b"\r");
            if crlf && self.record.is_empty() {
                out.extend_from_slice(part);
                continue;
            }
            self.record.extend_from_slice(part);
            if crlf {
                continue;
            }
            if part.ends_with(b"\r") || part.ends_with(b"\n") {
                if part.len() == 1 && !self.line_has_data {
                    if let Some(updated) = rewrite(&self.record, fallback) {
                        out.extend_from_slice(&updated);
                        self.record.clear();
                    } else {
                        out.append(&mut self.record);
                    }
                }
                self.line_has_data = false;
            } else {
                self.line_has_data = true;
            }
        }
        out.into()
    }
}

fn rewrite(record: &[u8], fallback: &str) -> Option<Bytes> {
    // Ordinary token deltas dominate this path.
    let text = std::str::from_utf8(record).ok()?;
    if !text.contains("\"error\"") {
        return None;
    }
    let mut decoder = SseDecoder::new(crate::channel::UNBOUNDED);
    let SseFrame::Event(mut event) = decoder.push(record).ok()?.pop()? else {
        return None;
    };
    event.data = retry::event(event.data.as_bytes(), fallback)?;
    gproxy_protocol::codec::encode_sse_event(&event, crate::channel::UNBOUNDED).ok()
}

pub(in crate::channels::codex) fn apply(
    body: HttpBody,
    fallback: Option<http::HeaderValue>,
) -> HttpBody {
    let upstream: ByteStream = match body {
        HttpBody::Bytes(bytes) => Box::pin(stream::iter([Ok(bytes)])),
        HttpBody::Stream(upstream) => upstream,
    };
    let mut records = Records::default();
    let chunks = upstream.map(Some).chain(stream::iter([None]));
    let fallback = fallback
        .and_then(|v| v.to_str().ok().map(str::to_owned))
        .unwrap_or_else(|| "0".into());
    HttpBody::Stream(Box::pin(chunks.flat_map(move |chunk| {
        let (bytes, error) = match chunk {
            Some(Ok(bytes)) => (records.push(&bytes, &fallback), None),
            rest => (
                std::mem::take(&mut records.record).into(),
                rest.and_then(Result::err),
            ),
        };
        let output = (!bytes.is_empty()).then_some(Ok(bytes));
        stream::iter(output.into_iter().chain(error.map(Err)))
    })))
}
