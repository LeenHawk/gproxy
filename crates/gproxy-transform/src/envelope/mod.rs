mod collector;
mod json_array;
mod response;
mod sse;
mod synthesize;

use bytes::Bytes;
use gproxy_protocol::OperationKey;

pub use collector::{BufferedResponse, ResponseCollector};
pub(crate) use response::Converter;
pub use response::ResponseStream;
pub use sse::{SseDecoder, SseFrame};
pub use synthesize::{synthesize_error, synthesize_keepalive, synthesize_response};

use crate::TransformError;

pub(crate) fn is_promotion(source: OperationKey, target: OperationKey) -> bool {
    let (
        gproxy_protocol::OperationKind::ContentGeneration(source_kind),
        gproxy_protocol::OperationKind::ContentGeneration(target_kind),
    ) = (source.kind(), target.kind())
    else {
        return false;
    };
    let source_semantic = semantic_kind(source_kind);
    let target_semantic = semantic_kind(target_kind);
    source_semantic == target_semantic
        && (source.operation() != target.operation() || source.kind() != target.kind())
}

pub(crate) fn promotion_request(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    use gproxy_protocol::{ContentGenerationKind as Kind, OperationKind::ContentGeneration};
    match (source.kind(), target.kind()) {
        (
            ContentGeneration(Kind::OpenAiResponsesWebSocket),
            ContentGeneration(Kind::OpenAiResponses),
        ) => {
            let request: gproxy_protocol::openai::ResponseWebSocketRequest =
                serde_json::from_slice(&body)?;
            let gproxy_protocol::openai::ResponseWebSocketRequest::ResponseCreate(mut request) =
                request
            else {
                return Err(TransformError::unsupported(
                    "Responses websocket",
                    "non-create frame",
                ));
            };
            request.response.stream = Some(true);
            Ok(Bytes::from(serde_json::to_vec(&request.response)?))
        }
        (
            ContentGeneration(Kind::OpenAiResponses),
            ContentGeneration(Kind::OpenAiResponsesWebSocket),
        ) => {
            let response: gproxy_protocol::openai::ResponseCreateRequest =
                serde_json::from_slice(&body)?;
            let request = crate::wire!(gproxy_protocol::openai::ResponseCreateWebSocketRequest {
                type_: gproxy_protocol::openai::ResponseCreateWebSocketRequestType::ResponseCreate,
                response,
                generate: None,
                client_metadata: None,
                rest: Default::default(),
            });
            Ok(Bytes::from(serde_json::to_vec(&request)?))
        }
        _ => Ok(body),
    }
}

pub(crate) fn promotion_response(body: Bytes) -> Result<Bytes, TransformError> {
    if serde_json::from_slice::<gproxy_protocol::openai::ResponseObject>(&body).is_ok()
        || serde_json::from_slice::<gproxy_protocol::openai::ResponseStreamEvent>(&body).is_ok()
    {
        Ok(body)
    } else {
        Err(TransformError::shape(
            "Responses envelope",
            "expected response object or stream event",
        ))
    }
}

fn semantic_kind(
    kind: gproxy_protocol::ContentGenerationKind,
) -> gproxy_protocol::ContentGenerationKind {
    match kind {
        gproxy_protocol::ContentGenerationKind::OpenAiResponsesWebSocket => {
            gproxy_protocol::ContentGenerationKind::OpenAiResponses
        }
        other => other,
    }
}
