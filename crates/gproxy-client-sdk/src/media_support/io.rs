use super::*;
use futures_core::Stream;
use futures_util::{StreamExt, stream};
use protocol::{
    HttpBody, WireResponse,
    codec::{CodecLimits, SseDecoder, SseFrame, read_http_body},
    connection::ByteStream,
};
use serde::de::DeserializeOwned;
use std::{collections::VecDeque, pin::Pin};

#[derive(Deserialize)]
enum ErrorTag {
    #[serde(rename = "error")]
    Error,
}
#[derive(Deserialize)]
#[serde(untagged, bound(deserialize = "T:Deserialize<'de>"))]
enum Record<T> {
    Error {
        error: protocol::Rest,
    },
    EventError {
        #[serde(rename = "type")]
        kind: ErrorTag,
    },
    Data(T),
}
pub(crate) fn parse<T: DeserializeOwned>(data: &str) -> Result<T> {
    match serde_json::from_str::<Record<T>>(data).map_err(Error::invalid)? {
        Record::Error { error } => {
            drop(error);
            Err(Error::from_event(data.into()))
        }
        Record::EventError {
            kind: ErrorTag::Error,
        } => Err(Error::from_event(data.into())),
        Record::Data(value) => Ok(value),
    }
}
pub(crate) struct Head {
    pub status: u16,
    pub headers: http::HeaderMap,
}
impl Head {
    pub fn metadata(&self) -> ResponseMetadata {
        ResponseMetadata {
            id: self
                .headers
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            model: self
                .headers
                .get("openai-model")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            timestamp: None,
        }
    }
    pub fn error(&self, error: Error) -> Error {
        error.with_head(self.status, &self.headers)
    }
}
pub(crate) async fn read<T>(
    response: WireResponse<HttpBody>,
    limits: CodecLimits,
    parse: impl FnOnce(&[u8], &Head) -> Result<T>,
) -> Result<T> {
    let head = Head {
        status: response.status.as_u16(),
        headers: response.headers,
    };
    let body = read_http_body(response.body, limits)
        .await
        .map_err(|e| head.error(e.into()))?;
    parse(&body, &head).map_err(|e| head.error(e.with_body(&body)))
}

#[cfg(not(target_arch = "wasm32"))]
type Events<E> = Pin<Box<dyn Stream<Item = E> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
type Events<E> = Pin<Box<dyn Stream<Item = E> + 'static>>;
/// Dropping or cancelling stops consuming the response. No background task runs.
pub struct MediaStream<E> {
    inner: Events<E>,
}
impl<E> MediaStream<E> {
    pub fn cancel(self) {
        drop(self);
    }
}
impl<E> Stream for MediaStream<E> {
    type Item = E;
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<E>> {
        self.inner.as_mut().poll_next(cx)
    }
}
impl<E: Send + 'static> MediaStream<E> {
    pub(crate) fn ready(events: Vec<E>) -> Self {
        Self {
            inner: Box::pin(stream::iter(events)),
        }
    }
}
pub(crate) enum Input {
    Data(String),
    End,
    Fault(Error),
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Step<E> = Box<dyn FnMut(Input, &Head) -> (Vec<E>, bool) + Send>;
#[cfg(target_arch = "wasm32")]
pub(crate) type Step<E> = Box<dyn FnMut(Input, &Head) -> (Vec<E>, bool)>;
pub(crate) fn sse<E: Send + 'static>(
    response: WireResponse<HttpBody>,
    limits: CodecLimits,
    initial: Vec<E>,
    step: Step<E>,
) -> MediaStream<E> {
    struct Reader<E> {
        bytes: ByteStream,
        sse: SseDecoder,
        head: Head,
        queue: VecDeque<E>,
        step: Step<E>,
        done: bool,
    }
    let bytes = match response.body {
        HttpBody::Stream(s) => s,
        HttpBody::Bytes(b) => Box::pin(stream::once(async { Ok(b) })) as ByteStream,
    };
    let reader = Reader {
        bytes,
        sse: SseDecoder::new(limits),
        head: Head {
            status: response.status.as_u16(),
            headers: response.headers,
        },
        queue: initial.into(),
        step,
        done: false,
    };
    let inner = stream::unfold(reader, |mut state| async move {
        loop {
            if let Some(event) = state.queue.pop_front() {
                return Some((event, state));
            }
            if state.done {
                return None;
            }
            let (frames, eof) = match state.bytes.next().await {
                Some(Ok(bytes)) => (state.sse.push(&bytes).map_err(Error::invalid), false),
                Some(Err(e)) => (Err(Error::network_cause(e.as_ref())), false),
                None => (state.sse.finish().map_err(Error::invalid), true),
            };
            match frames {
                Err(error) => {
                    let (events, _) =
                        (state.step)(Input::Fault(state.head.error(error)), &state.head);
                    state.queue.extend(events);
                    state.done = true;
                }
                Ok(frames) => {
                    let mut ended = eof;
                    for frame in frames {
                        match frame {
                            SseFrame::Done => ended = true,
                            SseFrame::Event(event) => {
                                let (events, done) =
                                    (state.step)(Input::Data(event.data), &state.head);
                                state.queue.extend(events);
                                state.done |= done;
                            }
                        }
                        if state.done {
                            break;
                        }
                    }
                    if ended && !state.done {
                        let (events, _) = (state.step)(Input::End, &state.head);
                        state.queue.extend(events);
                        state.done = true;
                    }
                }
            }
        }
    });
    MediaStream {
        inner: Box::pin(inner),
    }
}
pub(crate) fn interrupted() -> FinishReason {
    FinishReason::new(FinishReasonKind::Interrupted, None)
}
pub(crate) fn stop() -> FinishReason {
    FinishReason::new(FinishReasonKind::Stop, None)
}

pub(crate) fn is_json(headers: &http::HeaderMap) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|mime| {
            mime.split(';')
                .next()
                .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"))
        })
}
