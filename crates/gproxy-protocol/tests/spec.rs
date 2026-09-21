use std::collections::HashSet;

use gproxy_protocol::{
    Dialect, Operation, OperationKey,
    spec::{OPERATION_SPECS, OperationTransport},
};
use strum::IntoEnumIterator;

#[test]
fn every_declared_operation_has_a_unique_modelled_surface() {
    let mut keys = HashSet::new();
    for spec in OPERATION_SPECS {
        assert!(
            keys.insert(spec.key),
            "duplicate specification: {:?}",
            spec.key
        );
        if let OperationTransport::Http { request, response } = spec.transport {
            assert!(!request.is_empty());
            assert!(!response.is_empty());
        }
    }
    for operation in Operation::iter() {
        assert!(
            keys.iter().any(|key| key.operation == operation),
            "missing {operation:?}"
        );
    }
}

#[test]
fn websocket_operations_are_not_encoded_as_http_body_formats() {
    for key in [
        OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAiResponsesWebSocket,
        },
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiResponsesWebSocket,
        },
        OperationKey {
            operation: Operation::ConnectRealtime,
            dialect: Dialect::OpenAi,
        },
        OperationKey {
            operation: Operation::ConnectRealtime,
            dialect: Dialect::Gemini,
        },
    ] {
        let spec = OPERATION_SPECS.iter().find(|spec| spec.key == key).unwrap();
        assert_eq!(spec.transport, OperationTransport::WebSocket);
    }
}

#[test]
fn media_and_realtime_rows_match_wire_formats_without_claiming_codecs() {
    use gproxy_protocol::connection::StreamFraming as S;
    use gproxy_protocol::spec::HttpBodyFormat as F;
    let cases: &[(Operation, Dialect, &[F], &[F])] = &[
        (
            Operation::CreateImage,
            Dialect::OpenAi,
            &[F::Json],
            &[F::Json, F::JsonStream(S::Sse)],
        ),
        (
            Operation::EditImage,
            Dialect::OpenAi,
            &[F::Json, F::Multipart],
            &[F::Json, F::JsonStream(S::Sse)],
        ),
        (
            Operation::CreateSpeech,
            Dialect::OpenAi,
            &[F::Json],
            &[F::Binary, F::JsonStream(S::Sse)],
        ),
        (
            Operation::CreateTranscription,
            Dialect::OpenAi,
            &[F::Multipart],
            &[F::Json, F::Text, F::JsonStream(S::Sse)],
        ),
        (
            Operation::CreateTranslation,
            Dialect::OpenAi,
            &[F::Multipart],
            &[F::Json, F::Text],
        ),
        (
            Operation::CreateRealtimeCall,
            Dialect::OpenAi,
            &[F::Multipart],
            &[F::Text],
        ),
        (
            Operation::GuardianReview,
            Dialect::OpenAi,
            &[F::Json],
            &[F::JsonStream(S::Sse)],
        ),
        (
            Operation::GuardianClassify,
            Dialect::OpenAi,
            &[F::Json],
            &[F::JsonStream(S::Sse)],
        ),
        (
            Operation::StreamGenerateContent,
            Dialect::Gemini,
            &[F::Json],
            &[F::JsonStream(S::Sse), F::JsonStream(S::JsonArray)],
        ),
        (
            Operation::CreateFile,
            Dialect::Gemini,
            &[F::Multipart, F::Json, F::Binary],
            &[F::Json, F::Empty],
        ),
        (
            Operation::DeleteFile,
            Dialect::Gemini,
            &[F::Empty],
            &[F::Json],
        ),
        (
            Operation::CreateVideo,
            Dialect::OpenAi,
            &[F::Json, F::Multipart],
            &[F::Json],
        ),
        (
            Operation::DownloadVideoContent,
            Dialect::OpenAi,
            &[F::Empty],
            &[F::Binary],
        ),
    ];
    for (operation, dialect, request, response) in cases {
        let key = OperationKey {
            operation: *operation,
            dialect: *dialect,
        };
        let spec = OPERATION_SPECS.iter().find(|s| s.key == key).unwrap();
        assert_eq!(
            spec.transport,
            OperationTransport::Http { request, response }
        );
    }
    // No dialect cross-product is inferred: only actually modelled entries exist.
    assert!(!OPERATION_SPECS.iter().any(|s| s.key
        == OperationKey {
            operation: Operation::CreateSpeech,
            dialect: Dialect::Claude
        }));
    assert_eq!(OPERATION_SPECS.len(), 57);
}
