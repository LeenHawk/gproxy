mod json_array;
mod sse;

use std::sync::Arc;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, Frame, StreamDecoder, StreamEnd, StreamTail};

use self::json_array::JsonArrayCodec;
use self::sse::SseCodec;
use super::{CompiledRule, RuleModels, apply_response};

pub struct ResponseRuleDecoder {
    upstream: Option<Box<dyn StreamDecoder>>,
    codec: Codec,
    rules: Arc<[CompiledRule]>,
    operation: gproxy_protocol::OperationKey,
    primary_model: String,
    alternate_model: Option<String>,
    client_headers: http::HeaderMap,
}

enum Codec {
    Sse(SseCodec),
    JsonArray(JsonArrayCodec),
}

impl ResponseRuleDecoder {
    pub fn new(
        upstream: Option<Box<dyn StreamDecoder>>,
        rules: Arc<[CompiledRule]>,
        operation: gproxy_protocol::OperationKey,
        framing: gproxy_protocol::StreamFraming,
        models: RuleModels<'_>,
        client_headers: http::HeaderMap,
    ) -> Result<Self, ChannelError> {
        let codec = match framing {
            gproxy_protocol::StreamFraming::Sse => Codec::Sse(SseCodec::default()),
            gproxy_protocol::StreamFraming::JsonArray => {
                Codec::JsonArray(JsonArrayCodec::default())
            }
            gproxy_protocol::StreamFraming::WebSocket => {
                return Err(ChannelError::Decode(
                    "process response rules do not accept websocket framing".into(),
                ));
            }
        };
        let (primary_model, alternate_model) = models.owned();
        Ok(Self {
            upstream,
            codec,
            rules,
            operation,
            primary_model,
            alternate_model,
            client_headers,
        })
    }

    fn rewrite(&self, body: Bytes) -> Bytes {
        if body.as_ref() == b"[DONE]" {
            return body;
        }
        apply_response(
            &self.rules,
            self.operation,
            RuleModels::new(&self.primary_model, self.alternate_model.as_deref()),
            &self.client_headers,
            body,
        )
    }

    fn decode(&mut self, frames: Vec<Frame>) -> Result<Vec<Frame>, ChannelError> {
        let mut output = Vec::new();
        for frame in frames {
            let decoded = match &mut self.codec {
                Codec::Sse(codec) => codec.push(frame.0)?,
                Codec::JsonArray(codec) => codec.push(frame.0)?,
            };
            for frame in decoded {
                output.push(Frame(frame.map(|body| self.rewrite(body))));
            }
        }
        Ok(output)
    }
}

impl StreamDecoder for ResponseRuleDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let frames = match self.upstream.as_mut() {
            Some(upstream) => upstream.push(chunk)?,
            None => vec![Frame(chunk)],
        };
        self.decode(frames)
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        let mut tail = match self.upstream.as_mut() {
            Some(upstream) => upstream.finish(end)?,
            None => StreamTail::default(),
        };
        if end == StreamEnd::Interrupted {
            tail.frames.clear();
            return Ok(tail);
        }
        let mut frames = self.decode(std::mem::take(&mut tail.frames))?;
        let final_frames = match &mut self.codec {
            Codec::Sse(codec) => codec.finish()?,
            Codec::JsonArray(codec) => codec.finish()?,
        };
        for frame in final_frames {
            frames.push(Frame(frame.map(|body| self.rewrite(body))));
        }
        tail.frames = frames;
        Ok(tail)
    }
}

enum EncodedFrame {
    Sse { event: Option<String>, data: Bytes },
    Json { prefix: &'static [u8], data: Bytes },
    Raw(Bytes),
}

impl EncodedFrame {
    fn map(self, rewrite: impl FnOnce(Bytes) -> Bytes) -> Bytes {
        match self {
            Self::Sse { event, data } => sse::encode(event.as_deref(), rewrite(data)),
            Self::Json { prefix, data } => {
                let mut output = Vec::with_capacity(prefix.len() + data.len());
                output.extend_from_slice(prefix);
                output.extend_from_slice(&rewrite(data));
                Bytes::from(output)
            }
            Self::Raw(body) => body,
        }
    }
}
