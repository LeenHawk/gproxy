use super::*;
#[cfg(feature = "chat")]
use crate::dialects;
use futures_util::stream;
use gproxy_protocol::{
    HttpBody,
    codec::{SseDecoder, SseFrame},
    connection::ByteStream,
};
use std::collections::VecDeque;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub(crate) fn decode(
    body: HttpBody,
    request: &GenerateRequest,
    limits: gproxy_protocol::codec::CodecLimits,
    warnings: Vec<Warning>,
    http_status: u16,
    headers: http::HeaderMap,
) -> Result<GenerationStream> {
    #[cfg(feature = "chat")]
    let format = if request.model.dialect == Dialect::OpenaiChat {
        dialects::chat::output_format(request)?
    } else {
        None
    };
    #[cfg(not(feature = "chat"))]
    let format = None;
    struct Reader {
        bytes: ByteStream,
        sse: SseDecoder,
        decoder: Option<StreamDecoder>,
        queue: VecDeque<Event>,
        done: bool,
        http_status: u16,
        headers: http::HeaderMap,
    }
    let bytes = match body {
        HttpBody::Stream(bytes) => bytes,
        HttpBody::Bytes(bytes) => Box::pin(stream::once(async { Ok(bytes) })) as ByteStream,
    };
    let mut decoder = StreamDecoder::with_warnings(request.model.dialect, format, warnings);
    let queue = decoder.start().into();
    let state = Reader {
        bytes,
        sse: SseDecoder::new(limits),
        decoder: Some(decoder),
        queue,
        done: false,
        http_status,
        headers,
    };
    let inner = stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.queue.pop_front() {
                let event = match event {
                    Event::Error { error } => Event::Error {
                        error: error.with_head(state.http_status, &state.headers),
                    },
                    event => event,
                };
                if matches!(event, Event::Finish { .. } | Event::Error { .. }) {
                    state.done = true;
                }
                return Some((event, state));
            }
            if state.done {
                return None;
            }
            let (frames, eof) = match state.bytes.next().await {
                Some(Ok(bytes)) => (state.sse.push(&bytes).map_err(Error::invalid), false),
                Some(Err(error)) => (Err(Error::network_cause(error.as_ref())), false),
                None => (state.sse.finish().map_err(Error::invalid), true),
            };
            let ended = frames
                .as_ref()
                .is_ok_and(|f| f.iter().any(|f| matches!(f, SseFrame::Done)));
            // A transport EOF and a [DONE] both end framing, never synthesize a
            // successful vendor finish from a dropped connection.
            let result = (|| -> Result<()> {
                for frame in frames? {
                    if let SseFrame::Event(event) = frame {
                        state.queue.extend(
                            state
                                .decoder
                                .as_mut()
                                .ok_or_else(|| Error::invalid("event after stream completion"))?
                                .push_json(&event.data)?,
                        );
                    }
                }
                if ended || eof {
                    state.queue.extend(
                        state
                            .decoder
                            .take()
                            .ok_or_else(|| Error::invalid("stream already completed"))?
                            .finish()?,
                    );
                    state.done = true;
                }
                Ok(())
            })();
            if let Err(error) = result {
                let usage = state
                    .decoder
                    .as_ref()
                    .map(StreamDecoder::observed_usage)
                    .unwrap_or_default();
                state.queue.push_back(Event::Error { error });
                state.queue.push_back(Event::Finish {
                    reason: FinishReason::new(FinishReasonKind::Interrupted, None),
                    usage,
                });
                state.done = true;
            }
        }
    });
    Ok(GenerationStream {
        inner: Box::pin(inner),
    })
}
