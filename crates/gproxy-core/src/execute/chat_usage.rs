//! OpenAI Chat Completions streams report usage only when the request sets
//! `stream_options.include_usage`, and then in one extra chunk with no
//! choices. Metering needs that chunk from every Chat stream, so core asks for
//! it on every native send instead of leaving it to each channel, and hides it
//! again from a client that did not ask for it. The conversion path already
//! asks for it itself and drops the chunk on its own, so a converted request
//! arrives here with the option set and is left alone.

use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    connection::{ByteStream, Bytes},
};
use serde_json::Value;

/// How large one SSE event may grow while it is held for inspection. The usage
/// chunk is a few hundred bytes; anything larger than this cannot be it, so it
/// is released unread rather than buffered without bound.
const MAX_HELD_EVENT: usize = 64 * 1024;

/// Ask a streamed Chat request for its closing usage chunk. Returns true only
/// when core added the option, which is exactly when the client did not ask
/// and the chunk must be taken out of its response again.
///
/// A body that is not a JSON object, or whose `stream_options` is not an
/// object, is sent exactly as the client wrote it: metering must never be the
/// reason a request breaks. A streamed request body is left alone too, since
/// reading it here would mean buffering it.
pub(super) fn opt_in(operation: OperationKey, wire: &mut WireRequest<HttpBody>) -> bool {
    if operation.operation != Operation::StreamGenerateContent
        || operation.dialect != Dialect::OpenAiChat
    {
        return false;
    }
    let HttpBody::Bytes(bytes) = &wire.body else {
        return false;
    };
    let Ok(Value::Object(mut body)) = serde_json::from_slice::<Value>(bytes) else {
        return false;
    };
    let options = body
        .entry("stream_options")
        .or_insert_with(|| Value::Object(Default::default()));
    if options.is_null() {
        *options = Value::Object(Default::default());
    }
    let Some(options) = options.as_object_mut() else {
        return false;
    };
    if options.get("include_usage") == Some(&Value::Bool(true)) {
        return false;
    }
    options.insert("include_usage".into(), Value::Bool(true));
    match serde_json::to_vec(&Value::Object(body)) {
        Ok(encoded) => {
            wire.body = HttpBody::Bytes(Bytes::from(encoded));
            true
        }
        Err(_) => false,
    }
}

/// Take the usage-only chunk out of a successful streamed Chat response the
/// client did not ask usage from. The exchange has already observed the
/// upstream bytes by the time they reach this filter, so settlement still
/// sees the usage. An error, or a body that says it is JSON, is not a Chat
/// stream and passes untouched.
pub(super) fn strip(mut response: WireResponse<HttpBody>) -> WireResponse<HttpBody> {
    if !response.status.is_success() || declares_json(&response.headers) {
        return response;
    }
    response.body = match response.body {
        HttpBody::Bytes(bytes) => {
            let mut filter = UsageChunkFilter::default();
            let mut out = filter.push(&bytes);
            out.extend(filter.finish());
            if out.len() == bytes.len() {
                HttpBody::Bytes(bytes)
            } else {
                response.headers.remove(http::header::CONTENT_LENGTH);
                HttpBody::Bytes(Bytes::from(out))
            }
        }
        HttpBody::Stream(stream) => {
            response.headers.remove(http::header::CONTENT_LENGTH);
            HttpBody::Stream(filtered(stream))
        }
    };
    response
}

fn declares_json(headers: &http::HeaderMap) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("json"))
}

/// Run a byte stream through the filter chunk by chunk. Only the event still
/// being received is held back; everything complete goes out as it arrives.
fn filtered(inner: ByteStream) -> ByteStream {
    use futures_util::StreamExt;
    struct State {
        inner: Option<ByteStream>,
        filter: UsageChunkFilter,
    }
    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(inner),
            filter: UsageChunkFilter::default(),
        },
        |mut state| async move {
            loop {
                let inner = state.inner.as_mut()?;
                match inner.next().await {
                    Some(Ok(chunk)) => {
                        let out = state.filter.push(&chunk);
                        if out.is_empty() {
                            continue;
                        }
                        let out = if out.len() == chunk.len() {
                            chunk
                        } else {
                            Bytes::from(out)
                        };
                        return Some((Ok(out), state));
                    }
                    Some(Err(error)) => {
                        state.inner = None;
                        return Some((Err(error), state));
                    }
                    None => {
                        state.inner = None;
                        let tail = state.filter.finish();
                        if tail.is_empty() {
                            return None;
                        }
                        return Some((Ok(Bytes::from(tail)), state));
                    }
                }
            }
        },
    ))
}

/// A byte-preserving SSE splitter that drops whole usage-only events and
/// passes every other byte through unchanged. It tracks line ends itself
/// (`\n`, `\r\n` or `\r`, as SSE allows) instead of decoding frames, so
/// comments, `event:` and `id:` lines survive exactly as the upstream sent
/// them.
struct UsageChunkFilter {
    /// The event being received, while it may still turn out to be dropped.
    held: Vec<u8>,
    /// False once the current event outgrew [`MAX_HELD_EVENT`]; the rest of it
    /// streams straight through.
    holding: bool,
    /// No byte other than a line end has been seen on the current line.
    line_empty: bool,
    /// The previous byte was `\r`, so a following `\n` belongs to it.
    after_cr: bool,
    /// The event ended on a bare `\r`; its fate waits for the next byte to
    /// see whether a `\n` completes the terminator.
    ended: bool,
}

impl Default for UsageChunkFilter {
    fn default() -> Self {
        Self {
            held: Vec::new(),
            holding: true,
            line_empty: true,
            after_cr: false,
            ended: false,
        }
    }
}

impl UsageChunkFilter {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(chunk.len());
        for &byte in chunk {
            if self.ended {
                self.ended = false;
                if byte == b'\n' {
                    self.after_cr = false;
                    self.keep(byte, &mut out);
                    self.release(&mut out);
                    continue;
                }
                self.release(&mut out);
            }
            self.keep(byte, &mut out);
            match byte {
                b'\n' if self.after_cr => self.after_cr = false,
                b'\n' | b'\r' => {
                    self.after_cr = byte == b'\r';
                    if self.line_empty {
                        if byte == b'\r' {
                            self.ended = true;
                        } else {
                            self.release(&mut out);
                        }
                    }
                    self.line_empty = true;
                }
                _ => {
                    self.after_cr = false;
                    self.line_empty = false;
                }
            }
            if self.holding && self.held.len() > MAX_HELD_EVENT {
                out.append(&mut self.held);
                self.holding = false;
            }
        }
        out
    }

    /// An unterminated last event is not dispatched by any SSE reader, so it
    /// is handed on as it is rather than inspected.
    fn finish(&mut self) -> Vec<u8> {
        if self.ended {
            self.ended = false;
            let mut out = Vec::new();
            self.release(&mut out);
            return out;
        }
        std::mem::take(&mut self.held)
    }

    fn keep(&mut self, byte: u8, out: &mut Vec<u8>) {
        if self.holding {
            self.held.push(byte);
        } else {
            out.push(byte);
        }
    }

    /// The current event is complete: drop it if it is the usage chunk,
    /// otherwise pass it on, and start holding the next one.
    fn release(&mut self, out: &mut Vec<u8>) {
        if self.holding && !is_usage_only(&self.held) {
            out.extend_from_slice(&self.held);
        }
        self.held.clear();
        self.holding = true;
    }
}

/// The same test the protocol's stream adapter applies to a Chat chunk: no
/// choices and a non-null `usage`.
fn is_usage_only(event: &[u8]) -> bool {
    if !event.windows(7).any(|window| window == b"\"usage\"") {
        return false;
    }
    let Ok(text) = std::str::from_utf8(event) else {
        return false;
    };
    let mut data = String::new();
    for line in text.split(['\n', '\r']) {
        if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
        return false;
    };
    chunk
        .get("choices")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
        && chunk.get("usage").is_some_and(|usage| !usage.is_null())
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderMap, Method, StatusCode};

    const USAGE: &str = "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2}}";
    const CONTENT: &str = "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}],\"usage\":null}";

    fn chat_stream() -> OperationKey {
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiChat,
        }
    }

    fn wire(body: &'static str) -> WireRequest<HttpBody> {
        WireRequest {
            method: Method::POST,
            path: "/v1/chat/completions".into(),
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(body.as_bytes())),
        }
    }

    fn body(wire: &WireRequest<HttpBody>) -> &[u8] {
        match &wire.body {
            HttpBody::Bytes(bytes) => bytes,
            HttpBody::Stream(_) => panic!("buffered"),
        }
    }

    fn json(wire: &WireRequest<HttpBody>) -> Value {
        serde_json::from_slice(body(wire)).unwrap()
    }

    /// Feed `input` one split at a time, at every possible boundary, and check
    /// the output never depends on where the chunks were cut.
    fn filter_all_splits(input: &str) -> String {
        let bytes = input.as_bytes();
        let mut whole = UsageChunkFilter::default();
        let mut expected = whole.push(bytes);
        expected.extend(whole.finish());
        for cut in 0..=bytes.len() {
            let mut filter = UsageChunkFilter::default();
            let mut out = filter.push(&bytes[..cut]);
            out.extend(filter.push(&bytes[cut..]));
            out.extend(filter.finish());
            assert_eq!(out, expected, "cut at {cut}");
        }
        let mut filter = UsageChunkFilter::default();
        let mut out = Vec::new();
        for byte in bytes {
            out.extend(filter.push(std::slice::from_ref(byte)));
        }
        out.extend(filter.finish());
        assert_eq!(out, expected, "byte by byte");
        String::from_utf8(expected).unwrap()
    }

    #[test]
    fn a_chat_stream_is_asked_for_usage_and_remembers_the_client_did_not() {
        let mut request = wire(r#"{"model":"m","stream":true,"stream_options":{"other":1}}"#);
        assert!(opt_in(chat_stream(), &mut request));
        assert_eq!(
            json(&request)["stream_options"],
            serde_json::json!({"other": 1, "include_usage": true})
        );

        let mut request = wire(r#"{"model":"m","stream":true}"#);
        assert!(opt_in(chat_stream(), &mut request));
        assert_eq!(json(&request)["stream_options"]["include_usage"], true);

        let mut request = wire(r#"{"model":"m","stream_options":null}"#);
        assert!(opt_in(chat_stream(), &mut request));
        assert_eq!(json(&request)["stream_options"]["include_usage"], true);

        let mut request = wire(r#"{"model":"m","stream_options":{"include_usage":false}}"#);
        assert!(opt_in(chat_stream(), &mut request), "false is not asking");
        assert_eq!(json(&request)["stream_options"]["include_usage"], true);
    }

    #[test]
    fn a_client_that_asked_itself_is_left_as_it_is() {
        let raw = r#"{"model":"m","stream_options":{"include_usage":true}}"#;
        let mut request = wire(raw);
        assert!(!opt_in(chat_stream(), &mut request));
        assert_eq!(body(&request), raw.as_bytes(), "not even re-encoded");
    }

    #[test]
    fn bodies_it_cannot_read_pass_untouched() {
        for raw in [
            "not json",
            "[1,2]",
            r#"{"stream_options":"yes"}"#,
            r#"{"stream_options":[true]}"#,
        ] {
            let mut request = wire(raw);
            assert!(!opt_in(chat_stream(), &mut request), "{raw}");
            assert_eq!(body(&request), raw.as_bytes(), "{raw}");
        }
        let mut request = wire(r#"{"model":"m"}"#);
        request.body = HttpBody::Stream(Box::pin(futures_util::stream::empty()));
        assert!(!opt_in(chat_stream(), &mut request), "a streamed body");
    }

    #[test]
    fn other_operations_are_not_touched() {
        for operation in [
            OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            },
        ] {
            let raw = r#"{"model":"m","stream":true}"#;
            let mut request = wire(raw);
            assert!(!opt_in(operation, &mut request));
            assert_eq!(body(&request), raw.as_bytes());
        }
    }

    #[test]
    fn the_usage_chunk_is_dropped_at_any_chunk_boundary() {
        let input = format!(": keep-alive\n\n{CONTENT}\n\n{USAGE}\n\ndata: [DONE]\n\n");
        assert_eq!(
            filter_all_splits(&input),
            format!(": keep-alive\n\n{CONTENT}\n\ndata: [DONE]\n\n")
        );
    }

    #[test]
    fn crlf_and_cr_line_ends_are_understood() {
        let input = format!("{CONTENT}\r\n\r\n{USAGE}\r\n\r\ndata: [DONE]\r\n\r\n");
        assert_eq!(
            filter_all_splits(&input),
            format!("{CONTENT}\r\n\r\ndata: [DONE]\r\n\r\n")
        );
        let input = format!("{CONTENT}\r\r{USAGE}\r\rdata: [DONE]\r\r");
        assert_eq!(
            filter_all_splits(&input),
            format!("{CONTENT}\r\rdata: [DONE]\r\r")
        );
    }

    #[test]
    fn a_chunk_with_choices_keeps_its_usage() {
        let with_choices =
            "data: {\"choices\":[{\"index\":0,\"delta\":{}}],\"usage\":{\"prompt_tokens\":1}}\n\n";
        let null_usage = "data: {\"choices\":[],\"usage\":null}\n\n";
        let input = format!("{with_choices}{null_usage}event: x\ndata: {{\"usage\":1}}\n\n");
        assert_eq!(filter_all_splits(&input), input);
    }

    #[test]
    fn a_huge_event_streams_through_unread() {
        let big = format!(
            "data: {{\"choices\":[],\"usage\":{{\"pad\":\"{}\"}}}}\n\n",
            "x".repeat(MAX_HELD_EVENT)
        );
        let input = format!("{big}{USAGE}\n\n");
        let mut filter = UsageChunkFilter::default();
        let mut out = filter.push(input.as_bytes());
        out.extend(filter.finish());
        assert_eq!(
            String::from_utf8(out).unwrap(),
            big,
            "the big one is let through"
        );
    }

    #[test]
    fn an_unterminated_tail_is_handed_on() {
        assert_eq!(filter_all_splits(USAGE), USAGE);
    }

    #[tokio::test]
    async fn a_response_stream_is_filtered_and_errors_and_json_are_not() {
        use futures_util::StreamExt;
        let chunks = format!("{CONTENT}\n\n{USAGE}\n\ndata: [DONE]\n\n");
        let (head, tail) = chunks.split_at(CONTENT.len() + 20);
        let response = |status, content_type: Option<&'static str>| {
            let mut headers = HeaderMap::new();
            if let Some(value) = content_type {
                headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static(value),
                );
            }
            WireResponse {
                status,
                headers,
                body: HttpBody::Stream(Box::pin(futures_util::stream::iter([
                    Ok(Bytes::from(head.to_owned())),
                    Ok(Bytes::from(tail.to_owned())),
                ]))),
            }
        };
        async fn text(response: WireResponse<HttpBody>) -> String {
            let HttpBody::Stream(mut stream) = response.body else {
                panic!("stream")
            };
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.unwrap());
            }
            String::from_utf8(out).unwrap()
        }
        let stripped = text(strip(response(StatusCode::OK, Some("text/event-stream")))).await;
        assert_eq!(stripped, format!("{CONTENT}\n\ndata: [DONE]\n\n"));
        let untyped = text(strip(response(StatusCode::OK, None))).await;
        assert_eq!(untyped, stripped);
        let error = text(strip(response(StatusCode::BAD_REQUEST, None))).await;
        assert_eq!(error, chunks);
        let json = text(strip(response(StatusCode::OK, Some("application/json")))).await;
        assert_eq!(json, chunks);
    }
}
