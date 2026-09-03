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
