use super::*;
#[cfg(feature = "openai")]
#[test]
fn truncated_streams_are_interrupted_and_in_band_errors_finish_as_errors() {
    let decoder = stream::StreamDecoder::new(Dialect::Openai);
    let partial = support::aggregate(decoder.finish().unwrap());
    assert_eq!(partial.finish_reason.unified, FinishReasonKind::Interrupted);
    let mut decoder = stream::StreamDecoder::new(Dialect::Openai);
    let events=decoder.push_json(&json!({"type":"error","sequence_number":1,"code":"context_length_exceeded","message":"too long","param":null}).to_string()).unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Error {
            error: Error {
                kind: ErrorKind::ContextTooLong,
                ..
            }
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Finish {
            reason: FinishReason {
                unified: FinishReasonKind::Error,
                ..
            },
            ..
        }
    )));
}

#[cfg(feature = "claude")]
#[test]
fn mismatched_stream_deltas_and_invalid_indices_are_invalid_data() {
    let mut decoder = stream::StreamDecoder::new(Dialect::Claude);
    decoder.push_json(&json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}).to_string()).unwrap();
    assert!(matches!(decoder.push_json(&json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"wrong block"}}).to_string()),Err(Error {kind:ErrorKind::InvalidData,..})));
    let mut decoder = stream::StreamDecoder::new(Dialect::Claude);
    assert!(matches!(decoder.push_json(&json!({"type":"content_block_start","index":-1,"content_block":{"type":"text","text":""}}).to_string()),Err(Error {kind:ErrorKind::InvalidData,..})));
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn finish_preserves_raw_incomplete_values_and_partial_stream_content() {
    let mut native = answer();
    native["status"] = json!("incomplete");
    native["incomplete_details"] = json!({"reason":"max_output_tokens"});
    let result = dialects::openai::parse(serde_json::from_value(native).unwrap()).unwrap();
    assert_eq!(result.finish_reason.unified, FinishReasonKind::Length);
    assert_eq!(
        result.finish_reason.raw.as_deref(),
        Some("max_output_tokens")
    );
    let mut decoder = stream::StreamDecoder::new(Dialect::OpenaiChat);
    let mut events=decoder.push_json(&json!({"id":"partial","object":"chat.completion.chunk","created":1,"model":"served","choices":[{"index":0,"delta":{"content":"partial answer"},"finish_reason":null}]}).to_string()).unwrap();
    events.extend(decoder.finish().unwrap());
    let partial = support::aggregate(events);
    assert_eq!(partial.finish_reason.unified, FinishReasonKind::Interrupted);
    assert_eq!(partial.finish_reason.raw, None);
    assert!(
        matches!(&partial.content[0].content,Content::Text(Text {text,..}) if text=="partial answer")
    );
    let mut decoder = stream::StreamDecoder::new(Dialect::Openai);
    let mut failed = answer();
    failed["status"] = json!("failed");
    failed["error"] = json!({"code":"server_error","message":"busy"});
    let events = decoder
        .push_json(
            &json!({"type":"response.failed","sequence_number":1,"response":failed}).to_string(),
        )
        .unwrap();
    assert!(events.iter().any(|event|matches!(event,Event::Finish {reason:FinishReason {unified:FinishReasonKind::Error,raw:Some(raw)},..} if raw=="failed")));
}
