//! OpenAI Audio wire models.
//!
//! The local Speech page documents its JSON request but not the SSE event
//! payloads. The Transcription and Translation pages document responses and
//! multipart examples but omit a complete body-parameter table; request-only
//! fields not visible there are retained from the v2 public protocol model.

mod request;
mod response;
mod stream;

pub use request::*;
pub use response::*;
pub use stream::*;

/// Binary bytes returned when speech uses `stream_format: "audio"`.
pub type SpeechResponse = Vec<u8>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audio_models_round_trip_unknown_fields_and_events() {
        let request = json!({
            "input": "hello",
            "model": "tts-future",
            "voice": {"id":"voice_1", "future_voice":true},
            "response_format": "future_codec",
            "future_request": 1
        });
        let parsed: SpeechRequest = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), request);

        let response = json!({
            "text": "hello",
            "usage": {"type":"duration", "seconds":1.5, "future_usage":2},
            "future_response": {"x":1}
        });
        let parsed: TranscriptionResponse = serde_json::from_value(response.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), response);

        let verbose = json!({
            "task":"transcribe","language":"english","duration":1.5,
            "text":"hello","segments":[]
        });
        let parsed: TranscriptionResponse = serde_json::from_value(verbose).unwrap();
        assert!(matches!(parsed, TranscriptionResponse::Verbose(_)));

        let event = json!({"type":"transcript.future", "delta":"x", "future_event":true});
        let parsed: TranscriptionStreamEvent = serde_json::from_value(event.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), event);

        let speech = json!({"type":"speech.audio.delta", "audio":"AAEC", "future":1});
        let parsed: SpeechStreamEvent = serde_json::from_value(speech.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), speech);
    }
}
