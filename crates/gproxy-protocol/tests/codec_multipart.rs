use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_core::Stream;
use gproxy_protocol::{
    HttpBody,
    codec::{CodecErrorKind, CodecLimits, MultipartDecoder, MultipartEncoder, read_http_body},
    connection::{HeaderMap, HeaderValue, MultipartPart, TransportError},
};

fn limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 4096,
        max_value_bytes: 1024,
        max_body_bytes: 1024,
        max_line_bytes: 1024,
        max_part_bytes: 128,
        max_parts: 8,
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    for _ in 0..100_000 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
    panic!("multipart operation failed to make progress");
}

fn body(boundary: &str, complete: bool) -> Bytes {
    let ending = if complete {
        format!("--{boundary}--\r\n")
    } else {
        format!("--{boundary}")
    };
    Bytes::from(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\na\0b\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\nsecond\r\n{ending}"
    ))
}

#[test]
fn multipart_decoder_preserves_order_repeated_names_and_binary_bytes() {
    let mut decoder =
        MultipartDecoder::new(HttpBody::Bytes(body("BOUND", true)), "BOUND", limits()).unwrap();
    let first = ready(decoder.next_part()).unwrap().unwrap();
    let first_body = ready(read_http_body(first.body, limits())).unwrap();
    let second = ready(decoder.next_part()).unwrap().unwrap();
    assert_eq!(
        first.headers.get("content-disposition").unwrap(),
        "form-data; name=\"file\""
    );
    let second_body = ready(read_http_body(second.body, limits())).unwrap();
    assert_eq!(first_body, Bytes::from_static(b"a\0b"));
    assert_eq!(second_body, Bytes::from_static(b"second"));
    assert!(ready(decoder.next_part()).unwrap().is_none());
}

#[test]
fn multipart_rejects_truncated_input_and_part_limits() {
    let mut decoder =
        MultipartDecoder::new(HttpBody::Bytes(body("BOUND", false)), "BOUND", limits()).unwrap();
    let first = ready(decoder.next_part()).unwrap().unwrap();
    assert_eq!(
        ready(decoder.next_part()).unwrap_err().kind(),
        CodecErrorKind::Multipart
    );
    drop(first);
    let second = ready(decoder.next_part()).unwrap().unwrap();
    assert_eq!(
        ready(read_http_body(second.body, limits())).unwrap(),
        Bytes::from_static(b"second")
    );

    let mut truncated =
        MultipartDecoder::new(HttpBody::Bytes(body("BOUND", false)), "BOUND", limits()).unwrap();
    let first = ready(truncated.next_part()).unwrap().unwrap();
    assert!(ready(read_http_body(first.body, limits())).is_ok());
    let second = ready(truncated.next_part()).unwrap().unwrap();
    assert!(ready(read_http_body(second.body, limits())).is_ok());
    assert_eq!(
        ready(truncated.next_part()).unwrap_err().kind(),
        CodecErrorKind::UnexpectedEof
    );

    let tiny = CodecLimits {
        max_part_bytes: 2,
        ..limits()
    };
    let mut decoder =
        MultipartDecoder::new(HttpBody::Bytes(body("BOUND", true)), "BOUND", tiny).unwrap();
    let part = ready(decoder.next_part()).unwrap().unwrap();
    assert_eq!(
        ready(read_http_body(part.body, tiny)).unwrap_err().kind(),
        CodecErrorKind::Limit
    );

    let tiny_wire = CodecLimits {
        max_body_bytes: 10,
        ..limits()
    };
    let mut decoder =
        MultipartDecoder::new(HttpBody::Bytes(body("BOUND", true)), "BOUND", tiny_wire).unwrap();
    assert_eq!(
        ready(decoder.next_part()).unwrap_err().kind(),
        CodecErrorKind::Limit
    );
}

#[test]
fn multipart_encoder_round_trips_and_drops_current_body_without_reading_ahead() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-disposition",
        HeaderValue::from_static("form-data; name=\"file\""),
    );
    let mut encoder = MultipartEncoder::new(
        "BOUND",
        vec![MultipartPart {
            headers,
            body: HttpBody::Bytes(Bytes::from_static(b"abc")),
        }],
        limits(),
    )
    .unwrap();
    let mut output = Vec::new();
    while let Some(chunk) = ready(encoder.next_chunk()).unwrap() {
        output.extend_from_slice(&chunk);
    }
    assert_eq!(
        output,
        b"--BOUND\r\ncontent-disposition: form-data; name=\"file\"\r\n\r\nabc\r\n--BOUND--\r\n"
    );
    let tiny = CodecLimits {
        max_body_bytes: 4,
        ..limits()
    };
    let mut limited = MultipartEncoder::new(
        "BOUND",
        vec![MultipartPart {
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(b"x")),
        }],
        tiny,
    )
    .unwrap();
    assert_eq!(
        ready(limited.next_chunk()).unwrap_err().kind(),
        CodecErrorKind::Limit
    );

    let dropped = Arc::new(Mutex::new(false));
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-disposition",
        HeaderValue::from_static("form-data; name=\"stream\""),
    );
    let encoder = MultipartEncoder::new(
        "BOUND",
        vec![MultipartPart {
            headers,
            body: HttpBody::Stream(Box::pin(DropProbe {
                dropped: dropped.clone(),
            })),
        }],
        limits(),
    )
    .unwrap();
    // Constructing the first prefix does not poll the current body; dropping
    // the encoder then drops that body locally.
    let mut encoder = encoder;
    let _ = ready(encoder.next_chunk()).unwrap();
    drop(encoder);
    assert!(*dropped.lock().unwrap());
}

struct DropProbe {
    dropped: Arc<Mutex<bool>>,
}

impl Stream for DropProbe {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        panic!("multipart encoder polled dropped probe")
    }
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        *self.dropped.lock().unwrap() = true;
    }
}

#[test]
fn multipart_real_streams_work_for_every_split_and_after_decoder_drop() {
    use futures_util::stream;
    let wire = body("BOUND", true);
    for split in 0..=wire.len() {
        let input = HttpBody::Stream(Box::pin(stream::iter(vec![
            Ok(wire.slice(..split)),
            Ok(wire.slice(split..)),
        ])));
        let mut d = MultipartDecoder::new(input, "BOUND", limits()).unwrap();
        let p = ready(d.next_part()).unwrap().unwrap();
        assert_eq!(
            ready(read_http_body(p.body, limits())).unwrap(),
            b"a\0b".as_slice(),
            "split {split}"
        );
        let p = ready(d.next_part()).unwrap().unwrap();
        assert_eq!(
            ready(read_http_body(p.body, limits())).unwrap(),
            b"second".as_slice()
        );
        assert!(ready(d.next_part()).unwrap().is_none());
    }
    let chunks = wire
        .iter()
        .map(|b| Ok(Bytes::copy_from_slice(&[*b])))
        .collect::<Vec<_>>();
    let mut d = MultipartDecoder::new(
        HttpBody::Stream(Box::pin(stream::iter(chunks))),
        "BOUND",
        limits(),
    )
    .unwrap();
    let p = ready(d.next_part()).unwrap().unwrap();
    drop(d);
    assert_eq!(
        ready(read_http_body(p.body, limits())).unwrap(),
        b"a\0b".as_slice()
    );
}
#[test]
fn dropped_parts_are_skipped_under_limits_and_header_and_epilogue_limits_apply() {
    let mut d =
        MultipartDecoder::new(HttpBody::Bytes(body("BOUND", true)), "BOUND", limits()).unwrap();
    drop(ready(d.next_part()).unwrap());
    let p = ready(d.next_part()).unwrap().unwrap();
    assert_eq!(
        ready(read_http_body(p.body, limits())).unwrap(),
        b"second".as_slice()
    );
    assert!(ready(d.next_part()).unwrap().is_none());
    let mut d = MultipartDecoder::new(
        HttpBody::Bytes(body("BOUND", true)),
        "BOUND",
        CodecLimits {
            max_part_bytes: 2,
            ..limits()
        },
    )
    .unwrap();
    drop(ready(d.next_part()).unwrap());
    assert!(ready(d.next_part()).is_err());
    assert!(ready(d.next_part()).is_err());
    let wire = Bytes::from(format!(
        "--BOUND\r\nX-Long: {}\r\n\r\nx\r\n--BOUND--\r\n",
        "a".repeat(80)
    ));
    let mut d = MultipartDecoder::new(
        HttpBody::Bytes(wire),
        "BOUND",
        CodecLimits {
            max_buffer_bytes: 64,
            ..limits()
        },
    )
    .unwrap();
    assert_eq!(
        ready(d.next_part()).unwrap_err().kind(),
        CodecErrorKind::Limit
    );
    let mut d = MultipartDecoder::new(
        HttpBody::Bytes(body("BOUND", true)),
        "BOUND",
        CodecLimits {
            max_parts: 1,
            ..limits()
        },
    )
    .unwrap();
    drop(ready(d.next_part()).unwrap());
    assert_eq!(
        ready(d.next_part()).unwrap_err().kind(),
        CodecErrorKind::Limit
    );
    assert!(ready(d.next_part()).is_err());
}
#[test]
fn decoder_to_encoder_keeps_parts_streaming_without_collecting_file_bodies() {
    use futures_util::stream;
    use gproxy_protocol::connection::Multipart;
    let wire = body("BOUND", true);
    let chunks = wire
        .chunks(3)
        .map(|b| Ok(Bytes::copy_from_slice(b)))
        .collect::<Vec<_>>();
    let d = MultipartDecoder::new(
        HttpBody::Stream(Box::pin(stream::iter(chunks))),
        "BOUND",
        limits(),
    )
    .unwrap();
    let parts = Box::pin(stream::try_unfold(d, |mut decoder| async move {
        let part = decoder
            .next_part()
            .await
            .map_err(|e| Box::new(e) as TransportError)?;
        Ok(part.map(|part| (part, decoder)))
    }));
    let mut e = MultipartEncoder::from_multipart("OTHER", Multipart { parts }, limits()).unwrap();
    let mut output = vec![];
    while let Some(chunk) = ready(e.next_chunk()).unwrap() {
        output.extend_from_slice(&chunk);
    }
    let mut d =
        MultipartDecoder::new(HttpBody::Bytes(Bytes::from(output)), "OTHER", limits()).unwrap();
    let p = ready(d.next_part()).unwrap().unwrap();
    assert_eq!(
        ready(read_http_body(p.body, limits())).unwrap(),
        b"a\0b".as_slice()
    );
    let p = ready(d.next_part()).unwrap().unwrap();
    assert_eq!(
        ready(read_http_body(p.body, limits())).unwrap(),
        b"second".as_slice()
    );
}
#[cfg(target_arch = "wasm32")]
#[test]
fn wasm_decoder_accepts_non_send_input_and_yields_non_send_field_streams() {
    use futures_util::stream;
    use std::rc::Rc;
    let local = Rc::new(body("BOUND", true));
    let input = stream::once(async move { Ok((*local).clone()) });
    let mut decoder =
        MultipartDecoder::new(HttpBody::Stream(Box::pin(input)), "BOUND", limits()).unwrap();
    let part = ready(decoder.next_part()).unwrap().unwrap();
    assert_eq!(
        ready(read_http_body(part.body, limits())).unwrap(),
        b"a\0b".as_slice()
    );
}

#[test]
fn first_part_is_available_before_body_or_eof_and_body_drives_the_input() {
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Gated {
        step: u8,
        open: Arc<AtomicBool>,
    }
    impl Stream for Gated {
        type Item = Result<Bytes, TransportError>;
        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            match self.step {
                0 => {
                    self.step = 1;
                    Poll::Ready(Some(Ok(Bytes::from_static(
                        b"--B\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\n",
                    ))))
                }
                1 if !self.open.load(Ordering::SeqCst) => Poll::Pending,
                1 => {
                    self.step = 2;
                    Poll::Ready(Some(Ok(Bytes::from_static(b"payload\r\n--B--\r\n"))))
                }
                _ => Poll::Ready(None),
            }
        }
    }
    let open = Arc::new(AtomicBool::new(false));
    let mut d = MultipartDecoder::new(
        HttpBody::Stream(Box::pin(Gated {
            step: 0,
            open: open.clone(),
        })),
        "B",
        limits(),
    )
    .unwrap();
    let part = ready(d.next_part()).unwrap().unwrap();
    let HttpBody::Stream(mut body) = part.body else {
        panic!()
    };
    let mut next = body.next();
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(Pin::new(&mut next).poll(&mut cx).is_pending());
    open.store(true, Ordering::SeqCst);
    assert_eq!(ready(next).unwrap().unwrap(), b"payload".as_slice());
    assert!(ready(body.next()).is_none());
    assert!(ready(d.next_part()).unwrap().is_none());
}
#[test]
fn epilogue_and_input_errors_remain_bounded_and_never_yield_successful_eof() {
    use futures_util::stream;
    let wire = body("BOUND", true);
    let bounded = CodecLimits {
        max_body_bytes: wire.len() as u64,
        ..limits()
    };
    let mut d = MultipartDecoder::new(
        HttpBody::Stream(Box::pin(stream::iter(vec![
            Ok(wire),
            Ok(Bytes::from_static(b"x")),
        ]))),
        "BOUND",
        bounded,
    )
    .unwrap();
    for _ in 0..2 {
        let p = ready(d.next_part()).unwrap().unwrap();
        ready(read_http_body(p.body, limits())).unwrap();
    }
    assert_eq!(
        ready(d.next_part()).unwrap_err().kind(),
        CodecErrorKind::Limit
    );
    assert!(ready(d.next_part()).is_err());
    let header = Bytes::from_static(b"--B\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\n");
    let source: TransportError = Box::new(std::io::Error::other("origin failure"));
    let mut d = MultipartDecoder::new(
        HttpBody::Stream(Box::pin(stream::iter(vec![Ok(header), Err(source)]))),
        "B",
        limits(),
    )
    .unwrap();
    let part = ready(d.next_part()).unwrap().unwrap();
    let error = ready(read_http_body(part.body, limits())).unwrap_err();
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    let mut found = false;
    while let Some(e) = cause {
        found |= e.is::<std::io::Error>();
        cause = e.source();
    }
    assert!(found);
    assert!(ready(d.next_part()).is_err());
}
#[test]
fn encoder_failures_are_terminal_and_never_emit_a_normal_closing_boundary() {
    let mut e = MultipartEncoder::new(
        "B",
        vec![MultipartPart {
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(b"too long")),
        }],
        CodecLimits {
            max_part_bytes: 2,
            ..limits()
        },
    )
    .unwrap();
    assert!(ready(e.next_chunk()).unwrap().is_some());
    assert_eq!(
        ready(e.next_chunk()).unwrap_err().kind(),
        CodecErrorKind::Limit
    );
    assert!(ready(e.next_chunk()).is_err());
}
