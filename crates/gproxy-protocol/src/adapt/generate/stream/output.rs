use super::{event::NativeEvent, reader::SourceFraming};
use crate::{
    codec::{self, CodecLimits},
    transform::TransformError,
};
use bytes::Bytes;

pub(super) enum Encoder {
    WebSocket {
        total: u64,
        events: usize,
        limits: CodecLimits,
        lane: Option<String>,
    },
    Sse(codec::SseEncoder),
    Array(codec::JsonArrayEncoder),
    Ndjson(codec::NdjsonEncoder),
}

impl Encoder {
    pub fn new(framing: SourceFraming, limits: CodecLimits) -> Self {
        match framing {
            SourceFraming::Sse => Self::Sse(codec::SseEncoder::new(limits)),
            SourceFraming::JsonArray => Self::Array(codec::JsonArrayEncoder::new(limits)),
            SourceFraming::Ndjson => Self::Ndjson(codec::NdjsonEncoder::new(limits)),
        }
    }
    pub fn event<E: NativeEvent>(
        &mut self,
        event: &E,
        limits: CodecLimits,
    ) -> Result<Bytes, TransformError> {
        let value = match self {
            Self::WebSocket {
                total,
                events,
                limits: bound,
                lane,
            } => {
                #[derive(serde::Serialize)]
                struct Message<'a, E: serde::Serialize> {
                    #[serde(skip_serializing_if = "Option::is_none")]
                    stream_id: &'a Option<String>,
                    #[serde(flatten)]
                    event: &'a E,
                }
                let bytes = codec::encode_json(
                    &Message {
                        stream_id: lane,
                        event,
                    },
                    *bound,
                )
                .map_err(super::codec_error)?;
                *total = total
                    .checked_add(bytes.len() as u64)
                    .filter(|v| *v <= bound.max_body_bytes)
                    .ok_or_else(|| super::limit("WebSocket aggregate event bytes exceeded"))?;
                *events = events
                    .checked_add(1)
                    .ok_or_else(|| super::limit("WebSocket event count overflow"))?;
                Ok(bytes)
            }
            Self::Sse(encoder) => {
                let json = codec::encode_json(event, limits).map_err(super::codec_error)?;
                let data = std::str::from_utf8(&json).map_err(|e| {
                    TransformError::invalid_result("generation.stream", e.to_string())
                })?;
                encoder.event(&codec::SseEvent {
                    event: event.event_name().map(str::to_owned),
                    data: data.into(),
                    id: None,
                    retry: None,
                })
            }
            Self::Array(encoder) => encoder.push(event),
            Self::Ndjson(encoder) => encoder.encode(event),
        }
        .map_err(super::codec_error)?;
        if value.len() as u64 > limits.max_buffer_bytes {
            return Err(super::limit("encoded client event exceeds buffer limit"));
        }
        Ok(value)
    }
    pub fn finish<E: NativeEvent>(&mut self) -> Result<Bytes, TransformError> {
        match self {
            Self::Sse(encoder) if E::DONE => encoder.done().map_err(super::codec_error),
            Self::Array(encoder) => encoder.finish().map_err(super::codec_error),
            _ => Ok(Bytes::new()),
        }
    }
}

pub(super) fn headers(mut headers: http::HeaderMap, framing: SourceFraming) -> http::HeaderMap {
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(match framing {
            SourceFraming::Sse => "text/event-stream",
            SourceFraming::JsonArray => "application/json",
            SourceFraming::Ndjson => "application/x-ndjson",
        }),
    );
    headers
}
