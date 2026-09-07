use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use gproxy_channel_api::TransportError;

struct Inner {
    frames: VecDeque<Bytes>,
    closed: bool,
    waker: Option<Waker>,
}

/// Producer side of a detached synthesized stream: the upstream task pushes
/// frames, the response polls them. A client that disconnects merely leaves a
/// handful of frames unread; the task, and its settlement, run to completion.
pub(super) struct Sender(Arc<Mutex<Inner>>);

pub(super) struct Receiver(Arc<Mutex<Inner>>);

pub(super) fn channel() -> (Sender, Receiver) {
    let inner = Arc::new(Mutex::new(Inner {
        frames: VecDeque::new(),
        closed: false,
        waker: None,
    }));
    (Sender(inner.clone()), Receiver(inner))
}

impl Sender {
    pub(super) fn push(&self, frame: Bytes) {
        let mut inner = self.0.lock().expect("handoff lock");
        inner.frames.push_back(frame);
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }

    pub(super) fn close(&self) {
        let mut inner = self.0.lock().expect("handoff lock");
        inner.closed = true;
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}

impl futures_core::Stream for Receiver {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut inner = self.0.lock().expect("handoff lock");
        if let Some(frame) = inner.frames.pop_front() {
            return Poll::Ready(Some(Ok(frame)));
        }
        if inner.closed {
            return Poll::Ready(None);
        }
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
