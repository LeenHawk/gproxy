use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody,
    adapt::generate::stream::reader::*,
    codec::{CodecErrorKind, CodecLimits},
    wire::gemini::generate_content::GenerateContentResponseBody as Gemini,
};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
fn limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 4096,
        max_value_bytes: 4096,
        max_body_bytes: 32768,
        max_line_bytes: 4096,
        max_part_bytes: 4096,
        max_parts: 16,
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("reader drained past the available event"),
    }
}
fn content(value: NativeFrame<Gemini>) -> String {
    match value {
        NativeFrame::Event { value, .. } => serde_json::to_string(&value).unwrap(),
        _ => panic!("expected data event"),
    }
}
#[test]
fn each_framing_yields_before_transport_eof_and_cancellation_retains_state() {
    for (framing, first) in [
        (SourceFraming::Sse, "data: {\"responseId\":\"early\"}\n\n"),
        (SourceFraming::Ndjson, "{\"responseId\":\"early\"}\n"),
        (SourceFraming::JsonArray, "[{\"responseId\":\"early\"},"),
    ] {
        let body = futures_util::stream::once(async move { Ok(Bytes::from(first)) })
            .chain(futures_util::stream::pending());
        let mut reader = NativeReader::new(HttpBody::Stream(Box::pin(body)), framing, limits());
        assert!(content(ready(reader.next::<Gemini>()).unwrap().unwrap()).contains("early"));
        for _ in 0..2 {
            let mut waiting = Box::pin(reader.next::<Gemini>());
            assert!(
                waiting
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
    }
}
#[test]
fn utf8_chunking_rest_isolation_done_and_clean_eof() {
    let raw =
        "event: message\ndata: {\"responseId\":\"中文\",\"evil\":{\"x\":1}}\n\ndata: [DONE]\n\n";
    let chunks: Vec<_> = raw
        .as_bytes()
        .iter()
        .map(|b| Ok(Bytes::from(vec![*b])))
        .collect();
    let mut reader = NativeReader::new(
        HttpBody::Stream(Box::pin(futures_util::stream::iter(chunks))),
        SourceFraming::Sse,
        limits(),
    );
    let event = ready(reader.next::<Gemini>()).unwrap().unwrap();
    if let NativeFrame::Event { name, value } = event {
        assert_eq!(name.as_deref(), Some("message"));
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(encoded.contains("中文"));
        assert!(!encoded.contains("evil"));
    } else {
        panic!("missing data");
    }
    assert!(matches!(
        ready(reader.next::<Gemini>()).unwrap(),
        Some(NativeFrame::Done)
    ));
    assert!(ready(reader.next::<Gemini>()).unwrap().is_none());
}
#[test]
fn body_and_eof_failures_poison_reader() {
    for (framing, body, kind) in [
        (
            SourceFraming::Sse,
            "data: {}\n",
            CodecErrorKind::UnexpectedEof,
        ),
        (
            SourceFraming::JsonArray,
            "[{}",
            CodecErrorKind::UnexpectedEof,
        ),
        (SourceFraming::Sse, "data: {bad}\n\n", CodecErrorKind::Json),
    ] {
        let mut reader = NativeReader::new(HttpBody::Bytes(Bytes::from(body)), framing, limits());
        let error = loop {
            match ready(reader.next::<Gemini>()) {
                Ok(Some(_)) => {}
                Err(e) => break e,
                Ok(None) => panic!("accepted malformed body"),
            }
        };
        assert_eq!(error.kind(), kind);
        assert_eq!(
            ready(reader.next::<Gemini>()).unwrap_err().kind(),
            CodecErrorKind::Invalid
        );
    }
    let mut cap = limits();
    cap.max_body_bytes = 1;
    let mut reader = NativeReader::new(
        HttpBody::Bytes(Bytes::from_static(b"[]")),
        SourceFraming::JsonArray,
        cap,
    );
    assert_eq!(
        ready(reader.next::<Gemini>()).unwrap_err().kind(),
        CodecErrorKind::Limit
    );
}
#[test]
fn array_and_ndjson_multiple_values_preserve_order() {
    for (framing, body) in [
        (
            SourceFraming::JsonArray,
            "[{\"responseId\":\"a\"},{\"responseId\":\"b\"}]",
        ),
        (
            SourceFraming::Ndjson,
            "{\"responseId\":\"a\"}\n{\"responseId\":\"b\"}",
        ),
    ] {
        let mut reader = NativeReader::new(HttpBody::Bytes(Bytes::from(body)), framing, limits());
        for id in ["a", "b"] {
            let value = ready(reader.next::<Gemini>()).unwrap().unwrap();
            assert!(content(value).contains(&format!("\"responseId\":\"{id}\"")));
        }
        assert!(ready(reader.next::<Gemini>()).unwrap().is_none());
    }
}

#[test]
fn complete_array_prefix_is_yielded_before_malformed_later_value_in_same_chunk() {
    let mut reader = NativeReader::new(
        HttpBody::Bytes(Bytes::from_static(b"[{\"responseId\":\"first\"},,]")),
        SourceFraming::JsonArray,
        limits(),
    );
    let first = ready(reader.next::<Gemini>())
        .expect("later malformed value must not swallow completed prefix")
        .unwrap();
    assert!(content(first).contains("first"));
    assert_eq!(
        ready(reader.next::<Gemini>()).unwrap_err().kind(),
        CodecErrorKind::Invalid
    );
}
#[test]
fn ready_empty_transport_chunks_yield_cooperatively_instead_of_spinning() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let source = futures_util::stream::poll_fn(move |_| {
        if observed.fetch_add(1, Ordering::SeqCst) < 4096 {
            Poll::Ready(Some(Ok(Bytes::new())))
        } else {
            Poll::Pending
        }
    });
    let mut reader = NativeReader::new(
        HttpBody::Stream(Box::pin(source)),
        SourceFraming::Sse,
        limits(),
    );
    let mut future = Box::pin(reader.next::<Gemini>());
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert!(
        count.load(Ordering::SeqCst) <= 1024,
        "one reader poll consumed {} empty chunks",
        count.load(Ordering::SeqCst)
    );
}

#[test]
fn cancelled_partial_utf8_read_resumes_exact_payload_in_all_framings() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    for (framing, raw) in [
        (SourceFraming::Sse, "data: {\"responseId\":\"中文\"}\n\n"),
        (SourceFraming::Ndjson, "{\"responseId\":\"中文\"}\n"),
        (SourceFraming::JsonArray, "[{\"responseId\":\"中文\"}]"),
    ] {
        let split = raw.find('中').unwrap() + 1;
        let chunks = Arc::new(Mutex::new(std::collections::VecDeque::from([
            Bytes::copy_from_slice(&raw.as_bytes()[..split]),
        ])));
        let closed = Arc::new(AtomicBool::new(false));
        let source_chunks = chunks.clone();
        let source_closed = closed.clone();
        let source = futures_util::stream::poll_fn(move |_| {
            if let Some(chunk) = source_chunks.lock().unwrap().pop_front() {
                Poll::Ready(Some(Ok(chunk)))
            } else if source_closed.load(Ordering::SeqCst) {
                Poll::Ready(None)
            } else {
                Poll::Pending
            }
        });
        let mut reader = NativeReader::new(HttpBody::Stream(Box::pin(source)), framing, limits());
        {
            let mut next = Box::pin(reader.next::<Gemini>());
            assert!(
                next.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        chunks
            .lock()
            .unwrap()
            .push_back(Bytes::copy_from_slice(&raw.as_bytes()[split..]));
        closed.store(true, Ordering::SeqCst);
        assert!(content(ready(reader.next::<Gemini>()).unwrap().unwrap()).contains("中文"));
        assert!(ready(reader.next::<Gemini>()).unwrap().is_none());
    }
}
#[test]
fn transport_error_releases_stream_and_cannot_poll_it_again() {
    use std::{
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    struct FailedStream(Arc<AtomicUsize>);
    impl Drop for FailedStream {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl futures_core::Stream for FailedStream {
        type Item = Result<Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(Some(Err(
                std::io::Error::other("actual transport detail").into()
            )))
        }
    }
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut reader = NativeReader::new(
        HttpBody::Stream(Box::pin(FailedStream(dropped.clone()))),
        SourceFraming::Sse,
        limits(),
    );
    assert_eq!(
        ready(reader.next::<Gemini>()).unwrap_err().kind(),
        CodecErrorKind::Transport
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(
        ready(reader.next::<Gemini>()).unwrap_err().kind(),
        CodecErrorKind::Invalid
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}
