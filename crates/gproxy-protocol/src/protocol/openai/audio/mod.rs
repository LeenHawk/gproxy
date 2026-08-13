mod enums;
mod requests;
mod responses;
mod stream;

pub use enums::*;
pub use requests::*;
pub use responses::*;
pub use stream::*;

use super::common::OpenAiWireModel;

/// Speech responses are raw audio bytes unless `stream_format=sse` is used.
pub type SpeechWireModel = OpenAiWireModel<SpeechRequest, Vec<u8>>;
pub type TranscriptionWireModel = OpenAiWireModel<TranscriptionRequest, TranscriptionResponse>;
pub type TranscriptionStreamWireModel =
    OpenAiWireModel<TranscriptionRequest, TranscriptionStreamEvent>;
pub type TranslationWireModel = OpenAiWireModel<TranslationRequest, TranslationResponse>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcription_request_accepts_normalized_multipart_fields() {
        let request: TranscriptionRequest = serde_json::from_value(serde_json::json!({
            "file": "data:audio/wav;base64,UklGRg==",
            "model": "gpt-4o-transcribe-diarize",
            "chunking_strategy": {"type": "server_vad", "threshold": 0.5},
            "known_speaker_names": ["agent"],
            "response_format": "diarized_json",
            "stream": true
        }))
        .unwrap();
        assert!(request.stream.is_some_and(|stream| stream));
        assert!(matches!(
            request.chunking_strategy,
            Some(AudioChunkingStrategy::ServerVad(_))
        ));
    }

    #[test]
    fn transcription_response_selects_verbose_shape() {
        let response: TranscriptionResponse = serde_json::from_value(serde_json::json!({
            "task": "transcribe",
            "language": "english",
            "duration": 1.5,
            "text": "hello",
            "segments": []
        }))
        .unwrap();
        assert!(matches!(response, TranscriptionResponse::Verbose(_)));
    }

    #[test]
    fn changed_known_stream_event_falls_back_losslessly() {
        let event: TranscriptionStreamEvent = serde_json::from_value(serde_json::json!({
            "type": "transcript.text.delta",
            "delta": {"future": true}
        }))
        .unwrap();
        assert!(matches!(event, TranscriptionStreamEvent::Unknown(_)));
    }

    #[test]
    fn stream_done_accepts_official_usage_without_type_discriminator() {
        let event: TranscriptionStreamEvent = serde_json::from_value(serde_json::json!({
            "type": "transcript.text.done",
            "text": "hello",
            "usage": {"input_tokens": 7, "output_tokens": 3, "total_tokens": 10}
        }))
        .unwrap();
        assert!(matches!(event, TranscriptionStreamEvent::Known(_)));
    }
}
