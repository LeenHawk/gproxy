use bytes::Bytes;
use gproxy_channel_api::{StreamCtx, StreamDecoder, StreamEnd};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};

use super::CodexSseDecoder;

#[test]
fn interrupted_finish_never_synthesizes_a_terminal_event() {
    let mut decoder = CodexSseDecoder::for_operation(StreamCtx {
        key: OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        framing: gproxy_protocol::StreamFraming::Sse,
        request_body: &Bytes::new(),
        response_headers: &http::HeaderMap::new(),
    })
    .unwrap();
    decoder
        .push(Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"m1\",\"delta\":\"partial\"}\n\n",
        ))
        .unwrap();
    let tail = decoder.finish(StreamEnd::Interrupted).unwrap();
    assert!(tail.frames.is_empty());

    let mut truncated = CodexSseDecoder::for_operation(StreamCtx {
        key: OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        framing: gproxy_protocol::StreamFraming::Sse,
        request_body: &Bytes::new(),
        response_headers: &http::HeaderMap::new(),
    })
    .unwrap();
    truncated
        .push(Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"m1\",\"content_index\":0,\"delta\":\"partial\"}\n\n",
        ))
        .unwrap();
    assert!(truncated.finish(StreamEnd::Complete).is_err());
}
