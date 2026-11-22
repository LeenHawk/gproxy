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

pub fn converted(
    error: TransformError,
    dialect: Dialect,
    framing: SourceFraming,
    sequence: i64,
) -> TransportError {
    // gproxy's own conversion failures still reach the client as an error
    // event carrying the transform detail, not a bare connection close.
    let detail = crate::upstream_error::detail(&error).unwrap_or_else(|| {
        serde_json::json!({
            "type": "api_error",
            "code": "transform_error",
            "message": error.to_string(),
        })
    });
    let Some(value) = crate::upstream_error::event(
        &detail,
        crate::upstream_error::status(&detail),
        dialect,
        sequence,
    ) else {
        return crate::generate::transport(error);
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

pub fn into_event(error: TransportError) -> Result<Bytes, TransportError> {
    error.downcast::<StreamError>().map(|error| error.bytes)
}

pub fn is_error_frame(frame: &gproxy_protocol::codec::SseFrame) -> bool {
    let gproxy_protocol::codec::SseFrame::Event(event) = frame else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return false;
    };
    if (value["event_type"] == "interaction.completed"
        && matches!(
            value["interaction"]["status"].as_str(),
            Some("failed" | "cancelled")
        ))
        || (value["event_type"] == "interaction.status_update"
            && matches!(value["status"].as_str(), Some("failed" | "cancelled")))
    {
        return true;
    }
    match value["type"].as_str() {
        Some("error" | "response.failed") => true,
        None => value.get("error").is_some_and(Value::is_object),
        // Tool failures and metadata can carry errors without failing the turn.
        Some(_) => false,
    }
}

/// The upstream error a converted stream failed with, unwrapped to its error
/// object, when the failure is one (`converted`).
pub fn detail_of(error: &TransportError) -> Option<Value> {
    let error = error.downcast_ref::<StreamError>()?;
    crate::upstream_error::detail(&error.error)
}
