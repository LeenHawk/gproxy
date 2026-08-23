mod collector;
mod json_array;
mod response;
mod sse;

use bytes::Bytes;
use gproxy_protocol::OperationKey;

pub use collector::{BufferedResponse, ResponseCollector};
pub(crate) use response::Converter;
pub use response::ResponseStream;
pub(crate) use sse::{SseDecoder, SseFrame};

use crate::TransformError;

pub(crate) fn is_promotion(source: OperationKey, target: OperationKey) -> bool {
    matches!(
        (source.operation, source.kind, target.operation, target.kind),
        (
            gproxy_protocol::Operation::GenerateContent,
            gproxy_protocol::OperationKind::ContentGeneration(
                gproxy_protocol::ContentGenerationKind::OpenAiResponses
            ),
            gproxy_protocol::Operation::StreamGenerateContent,
            gproxy_protocol::OperationKind::ContentGeneration(
                gproxy_protocol::ContentGenerationKind::OpenAiResponses
            )
        )
    )
}

pub(crate) fn promotion_request(body: Bytes) -> Result<Bytes, TransformError> {
    let _: gproxy_protocol::openai::ResponseCreateRequest = serde_json::from_slice(&body)?;
    Ok(body)
}

pub(crate) fn promotion_response(body: Bytes) -> Result<Bytes, TransformError> {
    let _: gproxy_protocol::openai::ResponseObject = serde_json::from_slice(&body)?;
    Ok(body)
}
