use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, StreamFraming};

use super::SseFrame;
use crate::TransformError;

/// Convert one complete content-generation response into a strict stream.
pub fn synthesize_response(
    kind: Kind,
    body: Bytes,
    framing: StreamFraming,
) -> Result<Vec<Bytes>, TransformError> {
    match kind {
        Kind::OpenAiChat => {
            require_framing(framing, &[StreamFraming::Sse])?;
            let events = crate::typed::synthesize::openai_chat(serde_json::from_slice(&body)?);
            encode(events, framing, None, true)
        }
        Kind::OpenAiResponses | Kind::OpenAiResponsesWebSocket => {
            require_framing(framing, &[StreamFraming::Sse, StreamFraming::WebSocket])?;
            let events = crate::typed::synthesize::openai_responses(serde_json::from_slice(&body)?);
            encode(events, framing, Some(response_name), false)
        }
        Kind::ClaudeMessages => {
            require_framing(framing, &[StreamFraming::Sse])?;
            let events = crate::typed::synthesize::claude(serde_json::from_slice(&body)?);
            encode(events, framing, Some(claude_name), false)
        }
        Kind::GeminiGenerateContent => {
            require_framing(framing, &[StreamFraming::Sse, StreamFraming::JsonArray])?;
            let events = crate::typed::synthesize::gemini(serde_json::from_slice(&body)?);
            encode(events, framing, None, false)
        }
        #[cfg(not(feature = "exhaustive"))]
        _ => Err(TransformError::unsupported(
            "content generation kind",
            "unrecognized external variant",
        )),
    }
}

/// What keeps a client waiting on a synthesized stream from timing out while
/// the upstream is still producing the object: `None` when the framing has
/// no idle frame.
pub fn synthesize_keepalive(kind: Kind, framing: StreamFraming) -> Option<Bytes> {
    match framing {
        StreamFraming::JsonArray => Some(Bytes::from_static(b"\n")),
        StreamFraming::WebSocket => None,
        StreamFraming::Sse => Some(match kind {
            Kind::ClaudeMessages => SseFrame::encode(Some("ping"), r#"{"type":"ping"}"#),
            Kind::OpenAiChat
            | Kind::OpenAiResponses
            | Kind::OpenAiResponsesWebSocket
            | Kind::GeminiGenerateContent => Bytes::from_static(b": keep-alive\n\n"),
            #[cfg(not(feature = "exhaustive"))]
            _ => Bytes::from_static(b": keep-alive\n\n"),
        }),
    }
}

/// The failure a synthesized stream ends with once its headers are already
/// out: each protocol's own terminal error event.
pub fn synthesize_error(
    kind: Kind,
    framing: StreamFraming,
    message: &str,
) -> Result<Vec<Bytes>, TransformError> {
    let (event, value, done) = match kind {
        Kind::ClaudeMessages => (
            Some("error"),
            serde_json::json!({"type":"error","error":{"type":"api_error","message":message}}),
            false,
        ),
        Kind::OpenAiChat => (
            None,
            serde_json::json!({"error":{"type":"upstream_error","message":message}}),
            true,
        ),
        Kind::OpenAiResponses | Kind::OpenAiResponsesWebSocket => (
            Some("error"),
            serde_json::json!({"type":"error","code":"upstream_error","message":message,"param":null,"sequence_number":0}),
            false,
        ),
        Kind::GeminiGenerateContent => (
            None,
            serde_json::json!({"error":{"code":502,"status":"UNAVAILABLE","message":message}}),
            false,
        ),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(TransformError::unsupported(
                "content generation kind",
                "unrecognized external variant",
            ));
        }
    };
    match framing {
        StreamFraming::Sse => {
            let mut output = vec![SseFrame::typed(event, &value)?];
            if done {
                output.push(SseFrame::encode(None, "[DONE]"));
            }
            Ok(output)
        }
        StreamFraming::JsonArray => Ok(vec![Bytes::from(serde_json::to_vec(&[value])?)]),
        StreamFraming::WebSocket => Ok(vec![Bytes::from(serde_json::to_vec(&value)?)]),
    }
}

fn require_framing(
    framing: StreamFraming,
    supported: &[StreamFraming],
) -> Result<(), TransformError> {
    if supported.contains(&framing) {
        Ok(())
    } else {
        Err(TransformError::shape(
            "synthetic stream",
            "framing is not valid for the target protocol",
        ))
    }
}

fn encode<T: serde::Serialize>(
    events: Vec<T>,
    framing: StreamFraming,
    event_name: Option<fn(&T) -> Option<&str>>,
    done: bool,
) -> Result<Vec<Bytes>, TransformError> {
    match framing {
        StreamFraming::Sse => {
            let mut output = events
                .iter()
                .map(|event| SseFrame::typed(event_name.and_then(|name| name(event)), event))
                .collect::<Result<Vec<_>, _>>()?;
            if done {
                output.push(SseFrame::encode(None, "[DONE]"));
            }
            Ok(output)
        }
        StreamFraming::JsonArray => Ok(vec![Bytes::from(serde_json::to_vec(&events)?)]),
        StreamFraming::WebSocket => events
            .iter()
            .map(|event| {
                serde_json::to_vec(event)
                    .map(Bytes::from)
                    .map_err(Into::into)
            })
            .collect(),
    }
}

fn response_name(event: &gproxy_protocol::openai::ResponseStreamEvent) -> Option<&str> {
    event.event_name()
}

fn claude_name(event: &gproxy_protocol::claude::StreamEvent) -> Option<&str> {
    event.event_name()
}
