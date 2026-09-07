//! Pure pairwise wire transforms. Routing policy belongs to channels and core.
//!
//! Protocol extension bags stop at this crate's input boundary. A transform maps
//! only fields and enum values whose semantics it understands, emits empty target
//! extension bags, drops unknown optional objects and values, and rejects unknown
//! data required to produce a valid target shape. Cross-protocol correlation and
//! opaque continuation data use typed converter state or documented wire fields,
//! never private JSON metadata.
//! Same-wire operation and envelope promotions remain byte-preserving because
//! they do not perform a semantic conversion.

#[cfg(feature = "exhaustive")]
macro_rules! wire {
    ($value:expr) => {
        $value
    };
}

#[cfg(not(feature = "exhaustive"))]
macro_rules! wire {
    ($($tokens:tt)*) => {
        gproxy_protocol::wire!($($tokens)*)
    };
}

pub(crate) use wire;

mod common;
mod compact;
mod count_tokens;
mod embeddings;
mod envelope;
mod error;
mod generate_content;
mod images;
mod models;
mod registry;
pub mod typed;
mod videos;

use bytes::Bytes;
use gproxy_protocol::{OperationKey, StreamFraming};

pub use envelope::{
    BufferedResponse, ResponseCollector, ResponseStream, synthesize_error, synthesize_keepalive,
    synthesize_response,
};
pub use error::TransformError;
pub use gproxy_protocol as protocol;

pub fn can_transform(source: OperationKey, target: OperationKey) -> bool {
    envelope::is_promotion(source, target) || registry::resolve(source, target).is_some()
}

pub fn request(
    source: OperationKey,
    target: OperationKey,
    mut body: Bytes,
    upstream_model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    if envelope::is_promotion(source, target) {
        return envelope::promotion_request(source, target, body);
    }
    let original_target = target;
    let semantic_source = semantic_responses_key(source);
    let semantic_target = semantic_responses_key(target);
    if semantic_source != source {
        body = envelope::promotion_request(source, semantic_source, body)?;
    }
    let pair = registry::resolve(semantic_source, semantic_target).ok_or(
        TransformError::UnsupportedPair {
            source_key: source,
            target_key: target,
        },
    )?;
    let body = registry::request(pair, body, upstream_model, stream)?;
    if semantic_target != original_target {
        envelope::promotion_request(semantic_target, original_target, body)
    } else {
        Ok(body)
    }
}

pub fn response(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    if envelope::is_promotion(source, target) {
        return envelope::promotion_response(body);
    }
    let semantic_source = semantic_responses_key(source);
    let semantic_target = semantic_responses_key(target);
    let pair = registry::resolve(semantic_source, semantic_target).ok_or(
        TransformError::UnsupportedPair {
            source_key: source,
            target_key: target,
        },
    )?;
    registry::response(pair, body)
}

fn semantic_responses_key(key: OperationKey) -> OperationKey {
    if key.kind()
        == gproxy_protocol::OperationKind::ContentGeneration(
            gproxy_protocol::ContentGenerationKind::OpenAiResponsesWebSocket,
        )
    {
        return OperationKey::content(
            key.operation(),
            gproxy_protocol::ContentGenerationKind::OpenAiResponses,
        );
    }
    key
}

pub fn response_stream(
    source: OperationKey,
    target: OperationKey,
) -> Result<ResponseStream, TransformError> {
    ResponseStream::new(source, target)
}

pub fn response_stream_framed(
    source: OperationKey,
    target: OperationKey,
    source_framing: StreamFraming,
    target_framing: StreamFraming,
) -> Result<ResponseStream, TransformError> {
    ResponseStream::new_framed(source, target, source_framing, target_framing)
}

pub fn request_query(
    source: OperationKey,
    target: OperationKey,
    query: Option<&str>,
) -> Result<Option<String>, TransformError> {
    if source.operation() != gproxy_protocol::Operation::ListModels {
        return Ok(query.map(str::to_owned));
    }
    use gproxy_protocol::{OperationKind::Family, WireFamily};
    Ok(match (source.kind(), target.kind()) {
        (Family(WireFamily::Claude), Family(WireFamily::Gemini)) => {
            let values = query_pairs(query);
            let mut output = Vec::new();
            copy_query(&values, "limit", "pageSize", &mut output);
            copy_query(&values, "after_id", "pageToken", &mut output);
            joined_query(output)
        }
        (Family(WireFamily::Gemini), Family(WireFamily::Claude)) => {
            let values = query_pairs(query);
            let mut output = Vec::new();
            copy_query(&values, "pageSize", "limit", &mut output);
            copy_query(&values, "pageToken", "after_id", &mut output);
            joined_query(output)
        }
        (Family(WireFamily::Claude | WireFamily::Gemini), Family(WireFamily::OpenAi))
        | (Family(WireFamily::OpenAi), Family(WireFamily::Claude | WireFamily::Gemini)) => None,
        _ => query.map(str::to_owned),
    })
}

fn query_pairs(query: Option<&str>) -> Vec<(String, String)> {
    form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn copy_query(
    values: &[(String, String)],
    source: &str,
    target: &str,
    output: &mut Vec<(String, String)>,
) {
    if let Some((_, value)) = values.iter().find(|(key, _)| key == source) {
        output.push((target.into(), value.clone()));
    }
}

fn joined_query(values: Vec<(String, String)>) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(
            form_urlencoded::Serializer::new(String::new())
                .extend_pairs(values)
                .finish(),
        )
    }
}

#[cfg(test)]
mod tests;
