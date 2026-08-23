use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamDecoder, StreamEnd, StreamTail};
use gproxy_protocol::{OperationKey, StreamFraming};

pub(crate) struct TransformDecoder {
    upstream: Option<Box<dyn StreamDecoder>>,
    converter: gproxy_transform::ResponseStream,
}

impl TransformDecoder {
    pub(crate) fn new(
        source: OperationKey,
        target: OperationKey,
        source_framing: StreamFraming,
        target_framing: StreamFraming,
        upstream: Option<Box<dyn StreamDecoder>>,
    ) -> Self {
        Self {
            upstream,
            converter: gproxy_transform::response_stream_framed(
                source,
                target,
                source_framing,
                target_framing,
            )
            .expect("declared streaming transform remains wired"),
        }
    }

    fn convert(&mut self, frames: Vec<Frame>) -> Result<Vec<Frame>, ChannelError> {
        let mut output = Vec::new();
        for frame in frames {
            output.extend(
                self.converter
                    .push(frame.0)
                    .map_err(decode)?
                    .into_iter()
                    .map(Frame),
            );
        }
        Ok(output)
    }
}

impl StreamDecoder for TransformDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let frames = match self.upstream.as_mut() {
            Some(upstream) => upstream.push(chunk)?,
            None => vec![Frame(chunk)],
        };
        self.convert(frames)
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        let tail = match self.upstream.as_mut() {
            Some(upstream) => upstream.finish(end)?,
            None => StreamTail::default(),
        };
        if end == StreamEnd::Interrupted {
            return Ok(StreamTail {
                frames: Vec::new(),
                usage: tail.usage,
            });
        }
        let mut frames = self.convert(tail.frames)?;
        frames.extend(
            self.converter
                .finish()
                .map_err(decode)?
                .into_iter()
                .map(Frame),
        );
        Ok(StreamTail {
            frames,
            usage: tail.usage,
        })
    }
}

fn decode(error: gproxy_transform::TransformError) -> ChannelError {
    ChannelError::Decode(error.to_string())
}
