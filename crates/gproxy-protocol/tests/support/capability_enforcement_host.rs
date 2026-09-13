//! Deterministic test host; no real transport or timers.
use super::*;

pub(super) type TimedChunk = (Duration, Result<Bytes, TransportError>);

pub(super) fn block_on<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(std::task::Waker::noop());
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synchronous contract fake unexpectedly pending"),
    }
}

pub(super) fn request(body: HttpBody) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: "/upload".into(),
        query: None,
        headers: HeaderMap::new(),
        body,
    }
}

#[derive(Debug)]
pub(super) struct MidError;

impl fmt::Display for MidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("source stream failed")
    }
}

impl Error for MidError {}

pub(super) struct TimedChunks {
    pub(super) chunks: VecDeque<TimedChunk>,
    pub(super) clock: Arc<Mutex<Duration>>,
}

impl futures_core::Stream for TimedChunks {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Some((delay, item)) = self.chunks.pop_front() else {
            return Poll::Ready(None);
        };
        *self.clock.lock().unwrap() += delay;
        Poll::Ready(Some(item))
    }
}

pub(super) struct BoundedStream {
    pub(super) inner: TimedChunks,
    pub(super) limits: CapabilityLimits,
    pub(super) clock: Arc<Mutex<Duration>>,
    pub(super) started: Duration,
    pub(super) last_progress: Duration,
    pub(super) seen: u64,
    pub(super) done: bool,
}

impl BoundedStream {
    fn new(
        inner: TimedChunks,
        limits: CapabilityLimits,
        clock: Arc<Mutex<Duration>>,
        started: Duration,
    ) -> Self {
        let now = *clock.lock().unwrap();
        Self {
            inner,
            limits,
            clock,
            started,
            last_progress: now,
            seen: 0,
            done: false,
        }
    }

    fn failure(kind: CapabilityErrorKind, message: &'static str) -> TransportError {
        Box::new(CapabilityError::new(
            kind,
            CapabilityErrorStage::Stream,
            message,
        ))
    }
}

impl futures_core::Stream for BoundedStream {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        let now = *this.clock.lock().unwrap();
        if now.saturating_sub(this.started) >= this.limits.operation_total {
            this.done = true;
            this.inner.chunks.clear();
            return Poll::Ready(Some(Err(Self::failure(
                CapabilityErrorKind::Limit,
                "operation deadline exceeded",
            ))));
        }
        if now.saturating_sub(this.last_progress) >= this.limits.stream_idle {
            this.done = true;
            this.inner.chunks.clear();
            return Poll::Ready(Some(Err(Self::failure(
                CapabilityErrorKind::Limit,
                "stream idle deadline exceeded",
            ))));
        }
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let now = *this.clock.lock().unwrap();
                if now.saturating_sub(this.started) >= this.limits.operation_total {
                    this.done = true;
                    this.inner.chunks.clear();
                    return Poll::Ready(Some(Err(Self::failure(
                        CapabilityErrorKind::Limit,
                        "operation deadline exceeded",
                    ))));
                }
                if now.saturating_sub(this.last_progress) >= this.limits.stream_idle {
                    this.done = true;
                    this.inner.chunks.clear();
                    return Poll::Ready(Some(Err(Self::failure(
                        CapabilityErrorKind::Limit,
                        "stream idle deadline exceeded",
                    ))));
                }
                if this.seen.saturating_add(bytes.len() as u64) > this.limits.read_bytes {
                    this.done = true;
                    this.inner.chunks.clear();
                    return Poll::Ready(Some(Err(Self::failure(
                        CapabilityErrorKind::Limit,
                        "read byte limit exceeded",
                    ))));
                }
                this.seen += bytes.len() as u64;
                if !bytes.is_empty() {
                    this.last_progress = now;
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(source))) => {
                this.done = true;
                this.inner.chunks.clear();
                Poll::Ready(Some(Err(Box::new(CapabilityError::with_source(
                    CapabilityErrorKind::Transport,
                    CapabilityErrorStage::Stream,
                    "source stream failed",
                    source,
                )))))
            }
            Poll::Ready(None) => {
                this.done = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Clone)]
pub(super) struct BoundedUpstream {
    pub(super) limits: CapabilityLimits,
    pub(super) chunks: Arc<Mutex<VecDeque<TimedChunk>>>,
    pub(super) clock: Arc<Mutex<Duration>>,
}

impl Upstream for BoundedUpstream {
    type Target = ();

    fn send<'a>(
        &'a self,
        _: &'a Self::Target,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        let started = *self.clock.lock().unwrap();
        Box::pin(async move {
            match request.body {
                HttpBody::Bytes(bytes) => {
                    if bytes.len() as u64 > self.limits.write_bytes {
                        return Err(CapabilityError::new(
                            CapabilityErrorKind::Limit,
                            CapabilityErrorStage::BodyTransfer,
                            "write byte limit exceeded",
                        ));
                    }
                }
                HttpBody::Stream(mut input) => {
                    let mut seen = 0u64;
                    let mut last_progress = started;
                    while let Some(chunk) = input.next().await {
                        let chunk = chunk.map_err(|error| {
                            CapabilityError::with_source(
                                CapabilityErrorKind::Transport,
                                CapabilityErrorStage::BodyTransfer,
                                "request stream failed",
                                error,
                            )
                        })?;
                        let now = *self.clock.lock().unwrap();
                        if now.saturating_sub(started) >= self.limits.operation_total
                            || now.saturating_sub(last_progress) >= self.limits.stream_idle
                        {
                            return Err(CapabilityError::new(
                                CapabilityErrorKind::Limit,
                                CapabilityErrorStage::BodyTransfer,
                                "upload deadline exceeded",
                            ));
                        }
                        if chunk.len() as u64 > self.limits.write_bytes.saturating_sub(seen) {
                            return Err(CapabilityError::new(
                                CapabilityErrorKind::Limit,
                                CapabilityErrorStage::BodyTransfer,
                                "write byte limit exceeded",
                            ));
                        }
                        seen += chunk.len() as u64;
                        if !chunk.is_empty() {
                            last_progress = now;
                        }
                    }
                }
            }
            let chunks = self.chunks.lock().unwrap().drain(..).collect();
            let stream = BoundedStream::new(
                TimedChunks {
                    chunks,
                    clock: self.clock.clone(),
                },
                self.limits,
                self.clock.clone(),
                started,
            );
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: HttpBody::Stream(Box::pin(stream)),
            })
        })
    }

    fn connect<'a>(
        &'a self,
        _: &'a Self::Target,
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                "not implemented by bounded fake",
            ))
        })
    }

    fn limits(&self) -> CapabilityLimits {
        self.limits
    }
}

pub(super) fn bounded(chunks: Vec<TimedChunk>, limits: CapabilityLimits) -> BoundedUpstream {
    BoundedUpstream {
        limits,
        chunks: Arc::new(Mutex::new(chunks.into_iter().collect())),
        clock: Arc::new(Mutex::new(Duration::ZERO)),
    }
}

pub(super) struct DropStream {
    pub(super) dropped: Arc<Mutex<bool>>,
}

impl futures_core::Stream for DropStream {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}

impl Drop for DropStream {
    fn drop(&mut self) {
        *self.dropped.lock().unwrap() = true;
    }
}
