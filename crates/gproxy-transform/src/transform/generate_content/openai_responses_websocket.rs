//! Transport-level helpers for the OpenAI Responses WebSocket surface.
//!
//! The WebSocket request frame is a `response.create` object. Internally GPROXY
//! runs the existing HTTP Responses pipeline, so the frame is converted into a
//! normal `/v1/responses` JSON body. The response path strips SSE framing back
//! to plain JSON text messages, matching the Responses WebSocket wire shape.

use serde_json::Value;

use crate::transform::common::sse::SseDecoder;
use crate::transform::{TransformContext, TransformError};

/// Convert a normal HTTP Responses request body into a WebSocket
/// `response.create` text frame.
pub fn http_request_to_ws_request(
    mut value: Value,
    _ctx: &TransformContext,
) -> Result<Value, TransformError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| TransformError::InvalidInput {
            reason: "responses websocket request must be a JSON object".to_owned(),
        })?;
    object.insert(
        "type".to_owned(),
        Value::String("response.create".to_owned()),
    );
    Ok(value)
}

/// Convert a WebSocket `response.create` frame back into a normal HTTP
/// Responses request body.
pub fn ws_request_to_http_request(
    mut value: Value,
    _ctx: &TransformContext,
) -> Result<Value, TransformError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| TransformError::InvalidInput {
            reason: "websocket frame must be a JSON object".to_owned(),
        })?;
    let frame_type = object
        .remove("type")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| TransformError::InvalidInput {
            reason: "websocket frame missing type".to_owned(),
        })?;
    if frame_type != "response.create" {
        return Err(TransformError::InvalidInput {
            reason: format!("unsupported websocket frame type: {frame_type}"),
        });
    }
    object.insert("stream".to_owned(), Value::Bool(true));
    Ok(value)
}

pub fn identity(value: Value, _ctx: &TransformContext) -> Value {
    value
}

pub fn identity_result(value: Value, _ctx: &TransformContext) -> Result<Value, TransformError> {
    Ok(value)
}

/// Convert a downstream `response.create` WebSocket text frame into the JSON
/// body for an internal `POST /v1/responses` request.
///
/// Unknown fields are preserved intentionally: the WebSocket surface carries
/// Codex-specific fields such as `generate` and `client_metadata`, and future
/// OpenAI fields should not be lost by a typed round-trip.
pub fn response_create_frame_to_response_body(frame: &[u8]) -> Result<Vec<u8>, TransformError> {
    let mut value: Value =
        serde_json::from_slice(frame).map_err(|error| TransformError::InvalidInput {
            reason: format!("decode websocket frame: {error}"),
        })?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| TransformError::InvalidInput {
            reason: "websocket frame must be a JSON object".to_owned(),
        })?;
    let frame_type = object
        .remove("type")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| TransformError::InvalidInput {
            reason: "websocket frame missing type".to_owned(),
        })?;
    if frame_type != "response.create" {
        return Err(TransformError::InvalidInput {
            reason: format!("unsupported websocket frame type: {frame_type}"),
        });
    }

    object.insert("stream".to_owned(), Value::Bool(true));
    serde_json::to_vec(&value).map_err(|error| TransformError::Serialization {
        reason: error.to_string(),
    })
}

/// Validate a downstream Responses WebSocket frame without normalizing it.
pub fn validate_response_create_frame(frame: &[u8]) -> Result<(), TransformError> {
    let value: Value =
        serde_json::from_slice(frame).map_err(|error| TransformError::InvalidInput {
            reason: format!("decode websocket frame: {error}"),
        })?;
    let object = value
        .as_object()
        .ok_or_else(|| TransformError::InvalidInput {
            reason: "websocket frame must be a JSON object".to_owned(),
        })?;
    let frame_type = object
        .get("type")
        .and_then(|value| value.as_str())
        .ok_or_else(|| TransformError::InvalidInput {
            reason: "websocket frame missing type".to_owned(),
        })?;
    if frame_type != "response.create" {
        return Err(TransformError::InvalidInput {
            reason: format!("unsupported websocket frame type: {frame_type}"),
        });
    }
    Ok(())
}

/// Incrementally converts Responses SSE bytes into WebSocket text messages.
#[derive(Debug, Default)]
pub struct ResponseWebSocketSseDecoder {
    decoder: SseDecoder,
}

impl ResponseWebSocketSseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.decoder
            .push(chunk)
            .into_iter()
            .filter_map(frame_data)
            .collect()
    }

    pub fn finish(&mut self) -> Vec<String> {
        self.decoder
            .finish()
            .and_then(frame_data)
            .into_iter()
            .collect()
    }
}

fn frame_data(frame: crate::transform::common::sse::SseFrame) -> Option<String> {
    (frame.data.trim() != "[DONE]").then_some(frame.data)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    use super::*;

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponsesWebSocket,
            ),
        )
    }

    #[test]
    fn response_create_frame_becomes_streaming_responses_body() {
        let body = response_create_frame_to_response_body(
            br#"{"type":"response.create","model":"gpt-test","input":"hi","stream":false,"generate":false,"client_metadata":{"k":"v"},"future_field":{"x":1}}"#,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(value.get("type"), None);
        assert_eq!(value["model"], "gpt-test");
        assert_eq!(value["input"], "hi");
        assert_eq!(value["stream"], true);
        assert_eq!(value["generate"], false);
        assert_eq!(value["client_metadata"]["k"], "v");
        assert_eq!(value["future_field"], json!({ "x": 1 }));
    }

    #[test]
    fn http_request_becomes_response_create_frame() {
        let value = http_request_to_ws_request(
            json!({"model":"gpt-test","input":"hi","stream":true}),
            &ctx(),
        )
        .unwrap();

        assert_eq!(value["type"], "response.create");
        assert_eq!(value["model"], "gpt-test");
        assert_eq!(value["stream"], true);
    }

    #[test]
    fn rejects_non_create_frame() {
        let err = response_create_frame_to_response_body(br#"{"type":"session.update"}"#)
            .expect_err("unsupported frame should fail");
        assert!(err.to_string().contains("unsupported websocket frame type"));
    }

    #[test]
    fn sse_decoder_returns_plain_json_messages() {
        let mut decoder = ResponseWebSocketSseDecoder::new();
        let messages = decoder.push(
            b"event: response.created\ndata: {\"type\":\"response.created\"}\n\ndata: [DONE]\n\n",
        );
        assert_eq!(messages, vec![r#"{"type":"response.created"}"#]);
        assert!(decoder.finish().is_empty());
    }
}
