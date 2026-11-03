use super::*;
use futures_util::task::AtomicWaker;
use std::{
    collections::VecDeque,
    sync::Mutex,
    task::{Context, Poll},
};

#[derive(Default)]
pub(crate) struct Queue {
    events: Mutex<VecDeque<RealtimeEvent>>,
    waker: AtomicWaker,
}
impl Queue {
    pub fn push(&self, event: RealtimeEvent) {
        self.events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push_back(event);
        self.waker.wake();
    }
    pub fn pop(&self, cx: &mut Context<'_>) -> Poll<RealtimeEvent> {
        self.waker.register(cx.waker());
        match self
            .events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
        {
            Some(event) => Poll::Ready(event),
            None => Poll::Pending,
        }
    }
}
