use gproxy_protocol::codec::{
    CodecErrorKind, CodecLimits, SseDecoder, SseEvent, SseFrame, encode_sse_done, encode_sse_event,
};

fn limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 1024,
        max_value_bytes: 512,
        max_body_bytes: 4096,
        max_line_bytes: 512,
        max_part_bytes: 1024,
        max_parts: 8,
    }
}

#[test]
fn sse_preserves_event_id_data_retry_and_done_marker() {
    let mut decoder = SseDecoder::new(limits());
    let mut frames = Vec::new();
    for chunk in [
        b"event: message\nid: 7\nretry: 1500\ndata: {\"x\":".as_slice(),
        b"1}\n\n".as_slice(),
        b"data: [DONE]\n\n".as_slice(),
    ] {
        frames.extend(decoder.push(chunk).unwrap());
    }
    assert_eq!(
        frames,
        [
            SseFrame::Event(SseEvent {
                event: Some("message".into()),
                id: Some("7".into()),
                data: "{\"x\":1}".into(),
                retry: Some(1500),
            }),
            SseFrame::Done,
        ]
    );
}

#[test]
fn sse_handles_utf8_split_across_chunks_and_rejects_incomplete_eof() {
    let mut decoder = SseDecoder::new(limits());
    assert!(decoder.push(b"data: \xe2").unwrap().is_empty());
    let frames = decoder.push(b"\x82\xac\n\n").unwrap();
    assert_eq!(
        frames,
        [SseFrame::Event(SseEvent {
            event: None,
            id: None,
            data: "€".into(),
            retry: None,
        })]
    );
    let mut incomplete = SseDecoder::new(limits());
    incomplete.push(b"data: partial\n").unwrap();
    assert_eq!(
        incomplete.finish().unwrap_err().kind(),
        CodecErrorKind::UnexpectedEof
    );

    let mut invalid = SseDecoder::new(limits());
    // Invalid UTF-8 is rejected when the event is dispatched.
    assert_eq!(
        invalid.push(b"data: \xff\n\n").unwrap_err().kind(),
        CodecErrorKind::Utf8
    );
}

#[test]
fn sse_encoder_round_trips_framing() {
    let event = SseEvent {
        event: Some("delta".into()),
        id: Some("9".into()),
        data: "a\nb".into(),
        retry: Some(10),
    };
    let encoded = encode_sse_event(&event, limits()).unwrap();
    let mut decoder = SseDecoder::new(limits());
    assert_eq!(
        decoder.push(&encoded).unwrap(),
        [SseFrame::Event(event.clone())]
    );
    assert_eq!(encode_sse_done(), b"data: [DONE]\n\n".as_slice());
    let mut unsafe_event = event;
    unsafe_event.data = "line\rbreak".into();
    assert_eq!(
        encode_sse_event(&unsafe_event, limits())
            .unwrap_err()
            .kind(),
        CodecErrorKind::Invalid
    );
}

#[test]
fn sse_accepts_bom_cr_lines_ignores_nul_ids_and_terminates_after_error() {
    let mut decoder = SseDecoder::new(CodecLimits {
        max_buffer_bytes: 32,
        ..limits()
    });
    let frames = decoder
        .push(b"\xef\xbb\xbfevent: x\r\nid: a\0b\r\ndata: one\r\rdata: two\r\n\r\n")
        .unwrap();
    assert_eq!(
        frames,
        [
            SseFrame::Event(SseEvent {
                event: Some("x".into()),
                id: None,
                data: "one".into(),
                retry: None,
            }),
            SseFrame::Event(SseEvent {
                event: None,
                id: None,
                data: "two".into(),
                retry: None,
            })
        ]
    );
    let mut failed = SseDecoder::new(limits());
    assert!(failed.push(b"retry: nope\n\n").unwrap().is_empty());
    assert!(failed.finish().unwrap().is_empty());
}

#[test]
fn sse_crlf_bom_controls_and_done_are_split_invariant() {
    let wire=b"\xef\xbb\xbfid: first\r\nretry: 42\r\n\r\nevent: delta\r\ndata: a\r\ndata: b\r\n\r\nid: bad\0id\nretry: +3\nretry: nope\nretry: 99999999999999999999999\n\nevent: stop\ndata: [DONE]\n\nid:\r\rdata:\r\r";
    let expected = vec![
        SseFrame::Event(SseEvent {
            event: Some("delta".into()),
            id: Some("first".into()),
            data: "a\nb".into(),
            retry: Some(42),
        }),
        SseFrame::Done,
        SseFrame::Event(SseEvent {
            event: None,
            id: Some("".into()),
            data: "".into(),
            retry: Some(42),
        }),
    ];
    for split in 0..=wire.len() {
        let mut d = SseDecoder::new(limits());
        let mut out = d.push(&wire[..split]).unwrap();
        out.extend(d.push(&wire[split..]).unwrap());
        out.extend(d.finish().unwrap());
        assert_eq!(out, expected, "split {split}");
        assert_eq!(d.last_event_id(), Some(""));
        assert_eq!(d.retry(), Some(42));
    }
    let mut d = SseDecoder::new(limits());
    let mut out = vec![];
    for b in wire {
        out.extend(d.push(&[*b]).unwrap());
    }
    d.finish().unwrap();
    assert_eq!(out, expected);
}
#[test]
fn sse_limits_and_eof_never_create_spurious_events_or_continue_after_errors() {
    let mut d = SseDecoder::new(CodecLimits {
        max_buffer_bytes: 16,
        ..limits()
    });
    assert_eq!(
        d.push(b"data: a\n\ndata: b\n\ndata: c\n\n").unwrap().len(),
        3
    );
    d.finish().unwrap();
    assert!(d.push(b"data: d\n\n").is_err());
    let mut d = SseDecoder::new(limits());
    assert!(
        d.push(b"id: x\nretry: nope\nunknown: x\n\n")
            .unwrap()
            .is_empty()
    );
    assert!(d.finish().unwrap().is_empty());
    for incomplete in [b"data: x".as_slice(), b"data: x\n", b"data: [DONE]\r\n"] {
        let mut d = SseDecoder::new(limits());
        assert!(d.push(incomplete).unwrap().is_empty());
        assert_eq!(
            d.finish().unwrap_err().kind(),
            CodecErrorKind::UnexpectedEof
        );
    }
    let mut d = SseDecoder::new(CodecLimits {
        max_body_bytes: 9,
        ..limits()
    });
    assert!(d.push(b"data: x\n\n").is_ok());
    assert!(d.push(b"\n").is_err());
    assert!(d.finish().is_err());
    let mut d = SseDecoder::new(CodecLimits {
        max_line_bytes: 4,
        ..limits()
    });
    assert!(d.push(b"data:").is_err());
    let event = SseEvent {
        event: None,
        id: None,
        retry: None,
        data: "x".into(),
    };
    let mut e = gproxy_protocol::codec::SseEncoder::new(CodecLimits {
        max_body_bytes: 10,
        ..limits()
    });
    assert!(e.event(&event).is_ok());
    assert!(e.event(&event).is_err());
    assert!(e.done().is_err());
    for event in [
        SseEvent {
            event: Some("x\ndata: injected".into()),
            ..event.clone()
        },
        SseEvent {
            id: Some("x\0y".into()),
            ..event
        },
    ] {
        assert!(encode_sse_event(&event, limits()).is_err());
    }
}
