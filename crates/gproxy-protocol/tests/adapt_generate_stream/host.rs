use bytes::Bytes;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse, capability::*, connection::TransportError,
};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
pub fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    for _ in 0..10000 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("operation remained pending beyond cooperative parser yields");
}
pub fn limits() -> CapabilityLimits {
    CapabilityLimits {
        operation_total: Duration::from_secs(30),
        stream_idle: Duration::from_secs(5),
        read_bytes: 262144,
        write_bytes: 262144,
        ws_frame_bytes: 262144,
    }
}
#[derive(Default)]
pub struct Store {
    pub entries: Mutex<BTreeMap<String, StateEntry>>,
    pub gets: AtomicUsize,
    pub serial: AtomicUsize,
    pub hang_applied: AtomicBool,
    pub hung: AtomicBool,
    pub hang_key_prefix: Mutex<Option<String>>,
}
impl StateStore for Store {
    type Scope = ();
    fn get<'a>(
        &'a self,
        _: &'a (),
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move {
            self.gets.fetch_add(1, Ordering::SeqCst);
            Ok(self.entries.lock().unwrap().get(key).cloned())
        })
    }
    fn compare_exchange<'a>(
        &'a self,
        _: &'a (),
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let serial = self.serial.fetch_add(1, Ordering::SeqCst) + 1;
            let version = Version::from_bytes(serial.to_be_bytes().to_vec());
            {
                let mut entries = self.entries.lock().unwrap();
                if entries.get(key).map(|v| &v.version) != expected.as_ref() {
                    return Ok(CasResult::Conflict);
                }
                let replacement = replacement.unwrap();
                entries.insert(
                    key.into(),
                    StateEntry {
                        payload: replacement.payload,
                        version: version.clone(),
                        expires_at: replacement.expires_at,
                    },
                );
            }
            let keyed_hang = {
                let mut prefix = self.hang_key_prefix.lock().unwrap();
                if prefix
                    .as_ref()
                    .is_some_and(|prefix| key.starts_with(prefix))
                {
                    prefix.take();
                    true
                } else {
                    false
                }
            };
            if keyed_hang || self.hang_applied.swap(false, Ordering::SeqCst) {
                self.hung.store(true, Ordering::SeqCst);
                return std::future::pending().await;
            }
            Ok(CasResult::Applied(Some(version)))
        })
    }
    fn limits(&self) -> CapabilityLimits {
        limits()
    }
}
#[derive(Default)]
struct FeedInner {
    chunks: Mutex<VecDeque<Result<Bytes, TransportError>>>,
    closed: AtomicBool,
    waker: Mutex<Option<Waker>>,
    polls: AtomicUsize,
}
#[derive(Clone, Default)]
pub struct Feed(Arc<FeedInner>);
impl Feed {
    pub fn push(&self, bytes: impl Into<Bytes>) {
        self.0.chunks.lock().unwrap().push_back(Ok(bytes.into()));
        if let Some(waker) = self.0.waker.lock().unwrap().take() {
            waker.wake();
        }
    }
    pub fn close(&self) {
        self.0.closed.store(true, Ordering::SeqCst);
        if let Some(waker) = self.0.waker.lock().unwrap().take() {
            waker.wake();
        }
    }
    pub fn polls(&self) -> usize {
        self.0.polls.load(Ordering::SeqCst)
    }
}
impl futures_core::Stream for Feed {
    type Item = Result<Bytes, TransportError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.polls.fetch_add(1, Ordering::SeqCst);
        *self.0.waker.lock().unwrap() = Some(cx.waker().clone());
        if let Some(chunk) = self.0.chunks.lock().unwrap().pop_front() {
            Poll::Ready(Some(chunk))
        } else if self.0.closed.load(Ordering::SeqCst) {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}
pub struct Host {
    pub store: Arc<Store>,
    pub response: Mutex<Option<WireResponse<HttpBody>>>,
    pub sent: Mutex<Vec<WireRequest<Bytes>>>,
}
impl Host {
    pub fn stream(store: Arc<Store>, feed: Feed) -> Self {
        Self {
            store,
            response: Mutex::new(Some(WireResponse {
                status: http::StatusCode::OK,
                headers: http::HeaderMap::from_iter([(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("text/event-stream"),
                )]),
                body: HttpBody::Stream(Box::pin(feed)),
            })),
            sent: Mutex::new(Vec::new()),
        }
    }
}
/// Keys of the per-invocation reservations the stream no longer persists.
pub fn is_reservation(key: &str) -> bool {
    ["stream-invoke:", "stream-prepare:", "ws-connect:"]
        .iter()
        .any(|prefix| key.starts_with(prefix))
}
impl Upstream for Host {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            assert!(
                !self
                    .store
                    .entries
                    .lock()
                    .unwrap()
                    .keys()
                    .any(|k| is_reservation(k)),
                "a stream reservation reached the state store"
            );
            let HttpBody::Bytes(body) = request.body else {
                panic!("generation request must be JSON bytes")
            };
            self.sent.lock().unwrap().push(WireRequest {
                method: request.method,
                path: request.path,
                query: request.query,
                headers: request.headers,
                body,
            });
            Ok(self
                .response
                .lock()
                .unwrap()
                .take()
                .expect("unexpected second POST"))
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected WebSocket connect")
    }
    fn limits(&self) -> CapabilityLimits {
        limits()
    }
}
