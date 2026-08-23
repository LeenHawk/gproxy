//! Pure pairwise wire transforms. Routing policy belongs to channels and core.

mod common;
mod compact;
mod count_tokens;
mod envelope;
mod error;
mod generate_content;
mod models;
mod registry;

use bytes::Bytes;
use gproxy_protocol::OperationKey;

pub use envelope::{BufferedResponse, ResponseCollector, ResponseStream};
pub use error::TransformError;

pub fn can_transform(source: OperationKey, target: OperationKey) -> bool {
    envelope::is_promotion(source, target) || registry::resolve(source, target).is_some()
}

pub fn request(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
    upstream_model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    if envelope::is_promotion(source, target) {
        return envelope::promotion_request(body);
    }
    let pair = registry::resolve(source, target).ok_or(TransformError::UnsupportedPair {
        source_key: source,
        target_key: target,
    })?;
    registry::request(pair, body, upstream_model, stream)
}

pub fn response(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    if envelope::is_promotion(source, target) {
        return envelope::promotion_response(body);
    }
    let pair = registry::resolve(source, target).ok_or(TransformError::UnsupportedPair {
        source_key: source,
        target_key: target,
    })?;
    registry::response(pair, body)
}

pub fn response_stream(
    source: OperationKey,
    target: OperationKey,
) -> Result<ResponseStream, TransformError> {
    ResponseStream::new(source, target)
}

#[cfg(test)]
mod tests;
