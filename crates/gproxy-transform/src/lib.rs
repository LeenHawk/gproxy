//! Pure pairwise wire transforms. Routing policy belongs to channels and core.

mod compact;
mod content;
mod count_tokens;
mod error;
mod models;
mod pair;
mod stream;

use bytes::Bytes;
use gproxy_protocol::OperationKey;

pub use error::TransformError;
pub use stream::ResponseStream;

pub fn can_transform(source: OperationKey, target: OperationKey) -> bool {
    pair::resolve(source, target).is_some()
}

pub fn request(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
    upstream_model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    let pair = pair::resolve(source, target).ok_or(TransformError::UnsupportedPair {
        source_key: source,
        target_key: target,
    })?;
    pair::request(pair, body, upstream_model, stream)
}

pub fn response(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    let pair = pair::resolve(source, target).ok_or(TransformError::UnsupportedPair {
        source_key: source,
        target_key: target,
    })?;
    pair::response(pair, body)
}

pub fn response_stream(
    source: OperationKey,
    target: OperationKey,
) -> Result<ResponseStream, TransformError> {
    ResponseStream::new(source, target)
}

#[cfg(test)]
mod tests;
