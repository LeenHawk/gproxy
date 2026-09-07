use base64::Engine;
use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamCtx, StreamDecoder, StreamEnd, StreamTail};
use serde_json::Value;

pub(in crate::aws_bedrock) struct InvokeDecoder {
    parser: crate::shared::aws_eventstream::FrameParser,
    usage: crate::shared::claude::sse::ClaudeSseDecoder,
    stopped: bool,
}

impl InvokeDecoder {
    pub(in crate::aws_bedrock) fn new(ctx: StreamCtx<'_>) -> Self {
        Self {
            parser: Default::default(),
            usage: crate::shared::claude::sse::ClaudeSseDecoder::for_operation(ctx)
                .expect("Claude stream"),
            stopped: false,
        }
    }
}

impl StreamDecoder for InvokeDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let mut output = Vec::new();
        for frame in self.parser.push(chunk)? {
            if frame.exception_type.is_some() || frame.message_type.as_deref() == Some("exception")
            {
                return Err(ChannelError::Decode(format!(
                    "Bedrock {}",
                    frame
                        .exception_type
                        .as_deref()
                        .unwrap_or("stream exception")
                )));
            }
            if frame.event_type.as_deref() != Some("chunk") {
                continue;
            }
            let payload: Value = serde_json::from_slice(&frame.payload)
                .map_err(|error| ChannelError::Decode(error.to_string()))?;
            let encoded = payload
                .get("bytes")
                .and_then(Value::as_str)
                .ok_or_else(|| ChannelError::Decode("Bedrock chunk has no bytes".into()))?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| ChannelError::Decode(error.to_string()))?;
            let event: Value = serde_json::from_slice(&decoded)
                .map_err(|error| ChannelError::Decode(error.to_string()))?;
            self.stopped |= event["type"] == "message_stop";
            let wire = Bytes::from(format!("data: {event}\n\n"));
            output.extend(self.usage.push(wire)?);
        }
        Ok(output)
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        if end == StreamEnd::Complete {
            self.parser.finish()?;
            if !self.stopped {
                return Err(ChannelError::Decode(
                    "Bedrock stream ended before message_stop".into(),
                ));
            }
        }
        self.usage.finish(end)
    }
}
