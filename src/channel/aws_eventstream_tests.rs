use super::*;

#[test]
fn parses_fragmented_frames_once() {
    let frame = build_frame_with_headers(
        &[(":message-type", "event"), (":event-type", "chunk")],
        br#"{"bytes":"aGk="}"#,
    );
    let mut parser = SmithyFrameParser::new();
    let split = frame.len() / 2;
    assert!(parser.push(&frame[..split]).is_empty());
    let frames = parser.push(&frame[split..]);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].event_type.as_deref(), Some("chunk"));
    assert_eq!(frames[0].payload["bytes"], "aGk=");
    assert!(!parser.has_pending());
}

#[test]
fn distinguishes_eventstream_from_sse() {
    let frame = build_frame("chunk", br#"{}"#);
    assert_eq!(looks_like_frame(&frame[..3]), None);
    assert_eq!(looks_like_frame(&frame[..12]), Some(true));
    assert_eq!(looks_like_frame(b"data: {}\n\n"), Some(false));
}

#[test]
fn exposes_exception_type() {
    let frame = build_frame_with_headers(
        &[
            (":message-type", "exception"),
            (":exception-type", "throttlingException"),
        ],
        br#"{"message":"slow down"}"#,
    );
    let frame = SmithyFrameParser::new().push(&frame).remove(0);
    assert_eq!(frame.exception_type.as_deref(), Some("throttlingException"));
}
