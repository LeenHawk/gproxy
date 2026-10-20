//! Preserve upstream errors through failed settlement before writing client frames.
use gproxy_protocol::{
    Dialect,
    adapt::generate::stream::reader::SourceFraming,
    connection::{Bytes, TransportError},
    transform::TransformError,
};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
struct StreamError {
    error: TransformError,
    bytes: Bytes,
}

pub(super) fn converted(
    error: TransformError,
    dialect: Dialect,
    framing: SourceFraming,
    sequence: i64,
) -> TransportError {
    let Some(detail) = super::upstream_error::detail(&error) else {
        return super::generate::transport(error);
    };
    let Some(value) = super::upstream_error::event(
        &detail,
        super::upstream_error::status(&detail),
        dialect,
        sequence,
    ) else {
        return super::generate::transport(error);
    };
    let bytes = match framing {
        SourceFraming::Sse if dialect == Dialect::OpenAiChat => {
            Bytes::from(format!("data: {value}\n\ndata: [DONE]\n\n"))
        }
        SourceFraming::Sse => Bytes::from(format!("event: error\ndata: {value}\n\n")),
        SourceFraming::JsonArray => {
            let prefix = if sequence == 0 { "[" } else { "," };
            Bytes::from(format!("{prefix}{value}]"))
        }
        SourceFraming::Ndjson => Bytes::from(format!("{value}\n")),
    };
    tracing::warn!(%error, "upstream generation stream failed");
    Box::new(StreamError { error, bytes })
}

pub(crate) fn into_event(error: TransportError) -> Result<Bytes, TransportError> {
    error.downcast::<StreamError>().map(|error| error.bytes)
}

pub(crate) fn is_error_frame(frame: &gproxy_protocol::codec::SseFrame) -> bool {
    let gproxy_protocol::codec::SseFrame::Event(event) = frame else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return false;
    };
    match value["type"].as_str() {
        Some("error" | "response.failed") => true,
        None => value.get("error").is_some_and(Value::is_object),
        // Tool failures and metadata can carry errors without failing the turn.
        Some(_) => false,
    }
}
