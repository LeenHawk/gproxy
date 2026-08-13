use bytes::Bytes;
use http::{HeaderMap, Method};

use super::classify;
use crate::protocol::{Operation, OperationKind, Provider};

#[test]
fn paths_classify_with_transport_flags() {
    for (path, operation, body, streaming) in [
        (
            "/v1/audio/speech",
            Operation::CreateSpeech,
            br#"{"model":"tts-1","stream_format":"sse"}"#.as_slice(),
            true,
        ),
        (
            "/v1/audio/transcriptions",
            Operation::CreateTranscription,
            br#"{"model":"gpt-4o-transcribe","stream":"true"}"#.as_slice(),
            true,
        ),
        (
            "/v1/audio/translations",
            Operation::CreateTranslation,
            br#"{"model":"whisper-1"}"#.as_slice(),
            false,
        ),
    ] {
        let classified = classify(
            &Method::POST,
            path,
            &HeaderMap::new(),
            &Bytes::copy_from_slice(body),
        )
        .unwrap();
        assert_eq!(classified.op.operation(), operation);
        assert_eq!(
            classified.op.kind(),
            OperationKind::Provider(Provider::OpenAi)
        );
        assert_eq!(classified.stream, streaming);
    }
}
