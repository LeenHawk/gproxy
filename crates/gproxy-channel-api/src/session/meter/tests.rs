use bytes::Bytes;
use rust_decimal::Decimal;

use super::RealtimeMeter;
use crate::{SessionObservation, SessionUsage, SessionUsageKind, WsFrame};

fn meter() -> RealtimeMeter {
    RealtimeMeter::new(
        br#"{"sdp":"v=0","session":{"type":"realtime","model":"route","audio":{"input":{"transcription":{"model":"client-transcribe"}}}}}"#,
        "gpt-realtime",
    )
}

fn text(value: &str) -> WsFrame {
    WsFrame::Text(value.into())
}

fn usage(observation: SessionObservation) -> SessionUsage {
    let SessionObservation::Usage(usage) = observation else {
        panic!("expected metered usage");
    };
    usage
}

#[test]
fn response_done_preserves_every_token_detail() {
    let mut meter = meter();
    assert!(matches!(
        meter.observe(&text(
            r#"{"type":"session.created","session":{"type":"realtime","model":"gpt-realtime-actual","audio":{"input":{"transcription":{"model":"server-transcribe"}}}}}"#,
        )),
        SessionObservation::None
    ));
    assert!(meter.ready());
    let done = r#"{"type":"response.done","response":{"id":"resp_1","usage":{"total_tokens":18,"input_tokens":11,"output_tokens":7,"input_token_details":{"text_tokens":3,"audio_tokens":5,"image_tokens":2,"cached_tokens":1},"output_token_details":{"text_tokens":4,"audio_tokens":3}}}}"#;
    let sample = usage(meter.observe(&text(done)));
    assert_eq!(sample.kind, SessionUsageKind::Primary);
    assert_eq!(sample.model, "gpt-realtime-actual");
    assert_eq!(sample.usage.input_tokens, 11);
    assert_eq!(sample.usage.output_tokens, 7);
    assert_eq!(sample.usage.cached_input_tokens, 1);
    assert_eq!(sample.usage.metrics["cached_input_tokens"], Decimal::ONE);
    assert_eq!(sample.usage.metrics["text_input_tokens"], Decimal::from(3));
    assert_eq!(sample.usage.metrics["audio_input_tokens"], Decimal::from(5));
    assert_eq!(sample.usage.metrics["image_input_tokens"], Decimal::from(2));
    assert_eq!(sample.usage.metrics["text_output_tokens"], Decimal::from(4));
    assert_eq!(
        sample.usage.metrics["audio_output_tokens"],
        Decimal::from(3)
    );
    assert!(matches!(
        meter.observe(&text(done)),
        SessionObservation::None
    ));
}

#[test]
fn transcription_uses_its_server_model_and_own_usage_shape() {
    let mut meter = meter();
    assert!(matches!(
        meter.observe(&text(
            r#"{"type":"session.updated","session":{"type":"realtime","model":"gpt-realtime","audio":{"input":{"transcription":{"model":"gpt-4o-transcribe"}}}}}"#,
        )),
        SessionObservation::None
    ));
    let tokens = usage(meter.observe(&text(
            r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"i1","transcript":"hi","usage":{"type":"tokens","input_tokens":8,"output_tokens":2,"total_tokens":10,"input_token_details":{"audio_tokens":6,"text_tokens":2}}}"#,
        )));
    assert_eq!(tokens.kind, SessionUsageKind::Transcription);
    assert_eq!(tokens.model, "gpt-4o-transcribe");
    assert_eq!(tokens.usage.metrics["audio_input_tokens"], Decimal::from(6));

    let duration = usage(meter.observe(&text(
            r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"i2","transcript":"bye","usage":{"type":"duration","seconds":1.25}}"#,
        )));
    assert_eq!(
        duration.usage.metrics["audio_seconds"],
        Decimal::new(125, 2)
    );
}

#[test]
fn only_valid_server_usage_events_are_metered() {
    let mut meter = meter();
    for ignored in [
        r#"{"type":"response.hologram.delta","usage":{"input_tokens":999}}"#,
        r#"{"type":"session.update","usage":{"input_tokens":999}}"#,
    ] {
        assert!(matches!(
            meter.observe(&text(ignored)),
            SessionObservation::None
        ));
    }
    for compromised in [
        r#"{"type":"response.done","response":{"output":[]}}"#,
        r#"{"type":"response.done","future":true}"#,
        r#"{"type":"session.updated","future":true}"#,
    ] {
        assert!(matches!(
            meter.observe(&text(compromised)),
            SessionObservation::Compromised { .. }
        ));
    }
    assert!(matches!(
        meter.observe(&text(
            r#"{"type":"session.updated","session":{"type":"realtime","audio":{"input":{}}}}"#,
        )),
        SessionObservation::None
    ));
    assert!(matches!(
        meter.observe(&text(
                r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"i","transcript":"x","usage":{"type":"tokens","input_tokens":1,"output_tokens":0,"total_tokens":1}}"#,
            )),
        SessionObservation::Compromised { .. }
    ));
    assert!(matches!(
        meter.observe(&WsFrame::Binary(Bytes::from_static(b"x"))),
        SessionObservation::Compromised { .. }
    ));
}
