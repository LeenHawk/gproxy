mod blocks;
mod decode;
mod events;
mod finish;
pub(super) mod invoke;
mod wire;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamDecoder, StreamEnd, StreamTail};

pub(super) struct BedrockStreamDecoder {
    parser: crate::shared::aws_eventstream::FrameParser,
    state: events::State,
}

impl BedrockStreamDecoder {
    pub(super) fn new() -> Self {
        Self {
            parser: crate::shared::aws_eventstream::FrameParser::new(),
            state: events::State::default(),
        }
    }
}

impl StreamDecoder for BedrockStreamDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let mut output = Vec::new();
        for frame in self.parser.push(chunk)? {
            output.extend(self.state.handle(decode::frame(frame)?)?);
        }
        Ok(output)
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        if end == StreamEnd::Interrupted {
            return Ok(StreamTail {
                frames: Vec::new(),
                usage: self.state.normalized.take(),
                actual_service_tier: None,
            });
        }
        self.parser.finish()?;
        if !self.state.terminal
            || !self.state.started
            || !self.state.message_stopped
            || !self.state.metadata_seen
        {
            return Err(ChannelError::Decode(
                "Bedrock stream ended before messageStop and metadata".into(),
            ));
        }
        Ok(StreamTail {
            frames: Vec::new(),
            usage: self.state.normalized.take(),
            actual_service_tier: None,
        })
    }
}
