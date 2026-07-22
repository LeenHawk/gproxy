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
}
