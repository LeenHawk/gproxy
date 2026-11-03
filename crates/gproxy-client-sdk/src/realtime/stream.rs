use super::session::{Driver, State};
use super::*;
use futures_core::Stream;
use futures_util::{SinkExt, StreamExt, lock::Mutex, stream};
use protocol::connection::{WsFrame, WsReceiver};
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

#[cfg(not(target_arch = "wasm32"))]
type Events = Pin<Box<dyn Stream<Item = RealtimeEvent> + Send>>;
#[cfg(target_arch = "wasm32")]
type Events = Pin<Box<dyn Stream<Item = RealtimeEvent>>>;
/// Poll alongside sender operations. No runtime task is spawned by the SDK.
pub struct RealtimeStream {
    inner: Events,
}
impl Stream for RealtimeStream {
    type Item = RealtimeEvent;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
struct Pump {
    incoming: WsReceiver,
    state: Arc<Mutex<State>>,
    queue: Arc<queue::Queue>,
    pending: VecDeque<RealtimeEvent>,
    done: bool,
}
enum Input {
    Local(RealtimeEvent),
    Frame(Option<std::result::Result<WsFrame, protocol::connection::TransportError>>),
}
pub(super) fn make_stream(
    incoming: WsReceiver,
    state: Arc<Mutex<State>>,
    queue: Arc<queue::Queue>,
) -> RealtimeStream {
    let pump = Pump {
        incoming,
        state,
        queue,
        pending: VecDeque::new(),
        done: false,
    };
    RealtimeStream {
        inner: Box::pin(stream::unfold(pump, |mut pump| async move {
            loop {
                if let Some(event) = pump.pending.pop_front() {
                    return Some((event, pump));
                }
                if pump.done {
                    return None;
                }
                let input = futures_util::future::poll_fn(|cx| {
                    if let Poll::Ready(event) = pump.queue.pop(cx) {
                        return Poll::Ready(Input::Local(event));
                    }
                    pump.incoming.poll_next_unpin(cx).map(Input::Frame)
                })
                .await;
                let frame = match input {
                    Input::Local(event) => return Some((event, pump)),
                    Input::Frame(frame) => frame,
                };
                let result = pump.frame(frame).await;
                if let Err(error) = result {
                    let mut state = pump.state.lock().await;
                    state.closed = true;
                    pump.pending.push_back(RealtimeEvent::local(
                        state.vendor.clone(),
                        RealtimeEventKind::Error { error, fatal: true },
                    ));
                    pump.pending.push_back(RealtimeEvent::local(
                        state.vendor.clone(),
                        RealtimeEventKind::Closed {
                            code: None,
                            reason: "connection failed".into(),
                        },
                    ));
                    pump.done = true;
                }
            }
        })),
    }
}
impl Pump {
    async fn frame(
        &mut self,
        frame: Option<std::result::Result<WsFrame, protocol::connection::TransportError>>,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        let value: Value = match frame {
            Some(Ok(WsFrame::Text(text))) => serde_json::from_str(&text).map_err(Error::invalid)?,
            Some(Ok(WsFrame::Binary(bytes))) => {
                serde_json::from_slice(&bytes).map_err(Error::invalid)?
            }
            Some(Ok(WsFrame::Ping(bytes))) => {
                state
                    .outgoing
                    .send(WsFrame::Pong(bytes))
                    .await
                    .map_err(|e| Error::network_cause(e.as_ref()))?;
                return Ok(());
            }
            Some(Ok(WsFrame::Pong(_))) => return Ok(()),
            Some(Err(error)) => return Err(Error::network_cause(error.as_ref())),
            frame => {
                let (code, reason) = match frame {
                    Some(Ok(WsFrame::Close(Some(close)))) => (Some(close.code), close.reason),
                    Some(Ok(WsFrame::Close(None))) => (None, String::new()),
                    _ if state.closed => (Some(1000), "closed by caller".into()),
                    _ => (Some(1006), "connection ended without a close frame".into()),
                };
                if code.is_some_and(|c| !matches!(c, 1000 | 1001)) {
                    self.pending.push_back(RealtimeEvent::local(
                        state.vendor.clone(),
                        RealtimeEventKind::Error {
                            error: Error::network(format!("WebSocket closed: {code:?} {reason}")),
                            fatal: true,
                        },
                    ));
                }
                state.closed = true;
                self.done = true;
                self.pending.push_back(RealtimeEvent::local(
                    state.vendor.clone(),
                    RealtimeEventKind::Closed { code, reason },
                ));
                return Ok(());
            }
        };
        let kinds = match state.dialect {
            Driver::Openai => openai::decode(&mut state, value.clone())?,
            Driver::Gemini => gemini::decode(&mut state, value.clone())?,
        };
        if kinds.iter().any(|k| matches!(k, RealtimeEventKind::Ready)) {
            for frame in std::mem::take(&mut state.pending_history) {
                state.send(frame).await?;
            }
        }
        for kind in kinds {
            if matches!(kind, RealtimeEventKind::Error { fatal: true, .. }) {
                self.done = true;
                state.closed = true;
            }
            self.pending.push_back(RealtimeEvent {
                kind,
                metadata: RealtimeMetadata {
                    vendor: state.vendor.clone(),
                    native: Some(value.clone()),
                },
            });
        }
        if self.done {
            self.pending.push_back(RealtimeEvent::local(
                state.vendor.clone(),
                RealtimeEventKind::Closed {
                    code: None,
                    reason: "fatal native error".into(),
                },
            ));
        }
        Ok(())
    }
}
