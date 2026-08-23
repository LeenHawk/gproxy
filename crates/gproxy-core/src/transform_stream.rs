use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamDecoder, StreamTail};
use gproxy_protocol::OperationKey;

pub(crate) struct TransformDecoder {
    upstream: Option<Box<dyn StreamDecoder>>,
    converter: gproxy_transform::ResponseStream,
}

impl TransformDecoder {
    pub(crate) fn new(
        source: OperationKey,
        target: OperationKey,
        upstream: Option<Box<dyn StreamDecoder>>,
    ) -> Self {
        Self {
            upstream,
            converter: gproxy_transform::response_stream(source, target)
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

    fn finish(&mut self) -> Result<StreamTail, ChannelError> {
        let tail = match self.upstream.as_mut() {
            Some(upstream) => upstream.finish()?,
            None => StreamTail::default(),
        };
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
