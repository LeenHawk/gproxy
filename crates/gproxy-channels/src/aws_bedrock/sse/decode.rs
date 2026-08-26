use gproxy_channel_api::ChannelError;
use gproxy_protocol::aws::ConverseStreamEvent;
use serde::de::DeserializeOwned;

use crate::shared::aws_eventstream::Frame;

pub(super) fn frame(frame: Frame) -> Result<ConverseStreamEvent, ChannelError> {
    if frame
        .content_type
        .as_deref()
        .is_some_and(|value| value != "application/json")
    {
        return Err(decode("event payload is not application/json"));
    }
    let event_type = frame
        .exception_type
        .as_deref()
        .or(frame.event_type.as_deref())
        .ok_or_else(|| decode("frame has no event or exception type"))?;
    if frame
        .message_type
        .as_deref()
        .is_some_and(|kind| !matches!(kind, "event" | "exception"))
    {
        return Err(decode("unexpected event-stream message type"));
    }
    let payload = frame.payload;
    Ok(match event_type {
        "messageStart" => ConverseStreamEvent::MessageStart(parse(&payload)?),
        "contentBlockStart" => ConverseStreamEvent::ContentBlockStart(parse(&payload)?),
        "contentBlockDelta" => ConverseStreamEvent::ContentBlockDelta(parse(&payload)?),
        "contentBlockStop" => ConverseStreamEvent::ContentBlockStop(parse(&payload)?),
        "messageStop" => ConverseStreamEvent::MessageStop(parse(&payload)?),
        "metadata" => ConverseStreamEvent::Metadata(Box::new(parse(&payload)?)),
        "internalServerException" => ConverseStreamEvent::InternalServerException(parse(&payload)?),
        "modelStreamErrorException" => {
            ConverseStreamEvent::ModelStreamErrorException(parse(&payload)?)
        }
        "validationException" => ConverseStreamEvent::ValidationException(parse(&payload)?),
        "throttlingException" => ConverseStreamEvent::ThrottlingException(parse(&payload)?),
        "serviceUnavailableException" => {
            ConverseStreamEvent::ServiceUnavailableException(parse(&payload)?)
        }
        unknown => ConverseStreamEvent::Unknown {
            event_type: unknown.into(),
            payload: serde_json::from_slice(&payload)
                .map_err(|error| decode(format!("unknown event JSON: {error}")))?,
        },
    })
}

fn parse<T: DeserializeOwned>(payload: &[u8]) -> Result<T, ChannelError> {
    serde_json::from_slice(payload).map_err(|error| decode(format!("event payload JSON: {error}")))
}

fn decode(message: impl Into<String>) -> ChannelError {
    ChannelError::Decode(format!("Bedrock stream: {}", message.into()))
}
