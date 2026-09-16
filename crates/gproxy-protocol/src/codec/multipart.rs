use super::{CodecError, CodecErrorKind, CodecErrorStage, CodecFuture, CodecLimits};
use crate::connection::{
    ByteStream, HttpBody, Multipart, MultipartPart, PartStream, TransportError,
};
use bytes::Bytes;
use futures_core::Stream;
use futures_util::{StreamExt, stream};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

#[cfg(not(target_arch = "wasm32"))]
type Driver = Arc<Mutex<Pump>>;

#[cfg(target_arch = "wasm32")]
type Driver = std::rc::Rc<std::cell::RefCell<Pump>>;

#[cfg(not(target_arch = "wasm32"))]
fn access<T>(driver: &Driver, f: impl FnOnce(&mut Pump) -> T) -> T {
    f(&mut driver.lock().unwrap())
}

#[cfg(target_arch = "wasm32")]
fn access<T>(driver: &Driver, f: impl FnOnce(&mut Pump) -> T) -> T {
    f(&mut driver.borrow_mut())
}

fn failure(kind: CodecErrorKind, message: &'static str) -> CodecError {
    CodecError::new(kind, CodecErrorStage::Body, message)
}

fn map_error(source: multer::Error) -> CodecError {
    let kind = match &source {
        multer::Error::FieldSizeExceeded { .. } | multer::Error::StreamSizeExceeded { .. } => {
            CodecErrorKind::Limit
        }
        multer::Error::IncompleteStream
        | multer::Error::IncompleteFieldData { .. }
        | multer::Error::IncompleteHeaders => CodecErrorKind::UnexpectedEof,
        _ => CodecErrorKind::Multipart,
    };
    CodecError::with_source(
        kind,
        CodecErrorStage::Body,
        "multipart parsing failed",
        source,
    )
}

struct Feed {
    queue: VecDeque<Bytes>,
    closed: bool,
}

struct FeedStream(Arc<Mutex<Feed>>);

impl Stream for FeedStream {
    type Item = Result<Bytes, TransportError>;
    fn poll_next(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut f = self.0.lock().unwrap();
        if let Some(v) = f.queue.pop_front() {
            Poll::Ready(Some(Ok(v)))
        } else if f.closed {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

// Independent framing-size meter: multer parses values/headers; this meter
// prevents overlong headers from accumulating inside its private buffer.
#[derive(Clone, Copy)]
enum MeterMode {
    Preamble,
    Headers,
    Body,
    Boundary,
    End,
}

struct Meter {
    mode: MeterMode,
    line: Vec<u8>,
    marker: Vec<u8>,
    body_marker: Vec<u8>,
    tail: Vec<u8>,
    header_bytes: u64,
    limits: CodecLimits,
}

impl Meter {
    fn new(boundary: &str, limits: CodecLimits) -> Self {
        Self {
            mode: MeterMode::Preamble,
            line: Vec::new(),
            marker: format!("--{boundary}").into_bytes(),
            body_marker: format!("\r\n--{boundary}").into_bytes(),
            tail: Vec::new(),
            header_bytes: 0,
            limits,
        }
    }
    fn push(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        for &b in bytes {
            match self.mode {
                MeterMode::End => {}
                MeterMode::Body => {
                    self.tail.push(b);
                    while !self.body_marker.starts_with(&self.tail) {
                        self.tail.remove(0);
                    }
                    if self.tail.len() == self.body_marker.len() {
                        self.tail.clear();
                        self.line = self.marker.clone();
                        self.mode = MeterMode::Boundary;
                    }
                }
                MeterMode::Preamble | MeterMode::Boundary => {
                    self.line.push(b);
                    if self.line.starts_with(&self.marker)
                        && self.line.get(self.marker.len()..) == Some(b"--".as_slice())
                    {
                        self.mode = MeterMode::End;
                        self.line.clear();
                        continue;
                    }
                    if self.line.len() as u64 > self.limits.max_buffer_bytes {
                        return Err(failure(
                            CodecErrorKind::Limit,
                            "multipart preamble/boundary line exceeds buffer limit",
                        ));
                    }
                    if b == b'\n' {
                        let line = self
                            .line
                            .strip_suffix(b"\n")
                            .unwrap()
                            .strip_suffix(b"\r")
                            .unwrap_or_else(|| self.line.strip_suffix(b"\n").unwrap());
                        if let Some(suffix) = line.strip_prefix(self.marker.as_slice())
                            && suffix.iter().all(|b| matches!(b, b' ' | b'\t'))
                        {
                            self.mode = MeterMode::Headers;
                            self.header_bytes = 0;
                        }
                        self.line.clear();
                    }
                }
                MeterMode::Headers => {
                    self.header_bytes += 1;
                    if self.header_bytes > self.limits.max_buffer_bytes {
                        return Err(failure(
                            CodecErrorKind::Limit,
                            "multipart headers exceed buffer limit",
                        ));
                    }
                    self.line.push(b);
                    if b == b'\n' {
                        let line = self.line.strip_suffix(b"\n").unwrap();
                        let line = line.strip_suffix(b"\r").unwrap_or(line);
                        if line.len() as u64 > self.limits.max_line_bytes {
                            return Err(failure(
                                CodecErrorKind::Limit,
                                "multipart header line exceeds limit",
                            ));
                        }
                        if line.is_empty() {
                            self.mode = MeterMode::Body;
                        }
                        self.line.clear();
                    } else if self.line.len() as u64
                        > self
                            .limits
                            .max_line_bytes
                            .saturating_add(u64::from(b == b'\r'))
                    {
                        return Err(failure(
                            CodecErrorKind::Limit,
                            "multipart header line exceeds limit",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

struct Pump {
    input: Option<ByteStream>,
    pending: Bytes,
    feed: Arc<Mutex<Feed>>,
    seen: u64,
    limits: CodecLimits,
    quantum: usize,
    meter: Meter,
    error: Option<CodecError>,
}

impl Pump {
    fn close(&mut self) {
        self.input = None;
        self.pending = Bytes::new();
        self.feed.lock().unwrap().closed = true;
    }
    fn poll_feed(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if !self.pending.is_empty() {
            let n = self.quantum.min(self.pending.len());
            let bytes = self.pending.split_to(n);
            if let Err(e) = self.meter.push(&bytes) {
                self.error = Some(e);
                self.close();
            } else {
                self.feed.lock().unwrap().queue.push_back(bytes);
            }
            return Poll::Ready(());
        }
        let Some(input) = self.input.as_mut() else {
            self.feed.lock().unwrap().closed = true;
            return Poll::Ready(());
        };
        match input.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let n = self.seen.saturating_add(bytes.len() as u64);
                if n > self.limits.max_body_bytes {
                    self.error = Some(failure(
                        CodecErrorKind::Limit,
                        "multipart wire body exceeds limit",
                    ));
                    self.close();
                } else {
                    self.seen = n;
                    self.pending = bytes;
                }
                Poll::Ready(())
            }
            Poll::Ready(Some(Err(e))) => {
                self.error = Some(CodecError::with_source(
                    CodecErrorKind::Transport,
                    CodecErrorStage::Body,
                    "multipart input failed",
                    e,
                ));
                self.close();
                Poll::Ready(())
            }
            Poll::Ready(None) => {
                self.close();
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn poll_drain(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), CodecError>> {
        self.pending = Bytes::new();
        self.feed.lock().unwrap().queue.clear();
        if let Some(e) = self.error.take() {
            return Poll::Ready(Err(e));
        }
        for _ in 0..32 {
            let Some(input) = self.input.as_mut() else {
                return Poll::Ready(Ok(()));
            };
            match input.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    self.seen = self.seen.saturating_add(bytes.len() as u64);
                    if self.seen > self.limits.max_body_bytes {
                        self.close();
                        return Poll::Ready(Err(failure(
                            CodecErrorKind::Limit,
                            "multipart epilogue exceeds body limit",
                        )));
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    self.close();
                    return Poll::Ready(Err(CodecError::with_source(
                        CodecErrorKind::Transport,
                        CodecErrorStage::Body,
                        "multipart epilogue failed",
                        e,
                    )));
                }
                Poll::Ready(None) => {
                    self.close();
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

struct FieldStream {
    field: Option<multer::Field<'static>>,
    driver: Driver,
    failed: Arc<AtomicBool>,
    done: bool,
}

impl Stream for FieldStream {
    type Item = Result<Bytes, TransportError>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        for _ in 0..32 {
            if let Some(error) = access(&self.driver, |p| p.error.take()) {
                self.done = true;
                self.field = None;
                self.failed.store(true, Ordering::Release);
                return Poll::Ready(Some(Err(Box::new(error))));
            }
            let field = self.field.as_mut().expect("active field");
            match std::pin::Pin::new(field).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => return Poll::Ready(Some(Ok(bytes))),
                Poll::Ready(Some(Err(e))) => {
                    self.done = true;
                    self.field = None;
                    self.failed.store(true, Ordering::Release);
                    return Poll::Ready(Some(Err(Box::new(map_error(e)))));
                }
                Poll::Ready(None) => {
                    self.done = true;
                    self.field = None;
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    if access(&self.driver, |p| p.poll_feed(cx)).is_pending() {
                        return Poll::Pending;
                    }
                }
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// Each part owns a real field stream. Consume/drop it before next_part;
/// dropping an unread field lets multer discard it under the same size limits.
/// The caller driving next_part OR the returned body drives the original input.
/// On wasm only the private queue is Send; the original stream stays !Send.
pub struct MultipartDecoder {
    inner: multer::Multipart<'static>,
    driver: Driver,
    limits: CodecLimits,
    parts: usize,
    failed: Arc<AtomicBool>,
    done: bool,
}

impl MultipartDecoder {
    pub fn new(
        body: HttpBody,
        boundary: impl Into<String>,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        let boundary = boundary.into();

        let reserve = boundary.len() + 8;
        let quantum = usize::try_from(limits.max_buffer_bytes)
            .unwrap_or(usize::MAX)
            .checked_sub(reserve)
            .filter(|q| *q > 0)
            .ok_or_else(|| {
                failure(
                    CodecErrorKind::Limit,
                    "multipart buffer cannot hold boundary lookbehind",
                )
            })?
            .min(4096);
        let input: ByteStream = match body {
            HttpBody::Bytes(b) => Box::pin(stream::once(async move { Ok(b) })),
            HttpBody::Stream(s) => s,
        };
        let feed = Arc::new(Mutex::new(Feed {
            queue: VecDeque::new(),
            closed: false,
        }));
        let inner = multer::Multipart::with_constraints(
            FeedStream(feed.clone()),
            &boundary,
            multer::Constraints::new().size_limit(
                multer::SizeLimit::new()
                    .whole_stream(limits.max_body_bytes)
                    .per_field(limits.max_part_bytes),
            ),
        );
        let pump = Pump {
            input: Some(input),
            pending: Bytes::new(),
            feed,
            seen: 0,
            limits,
            quantum,
            meter: Meter::new(&boundary, limits),
            error: None,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let driver = Arc::new(Mutex::new(pump));
        #[cfg(target_arch = "wasm32")]
        let driver = std::rc::Rc::new(std::cell::RefCell::new(pump));
        Ok(Self {
            inner,
            driver,
            limits,
            parts: 0,
            failed: Arc::new(AtomicBool::new(false)),
            done: false,
        })
    }
    pub fn next_part(&mut self) -> CodecFuture<'_, Result<Option<MultipartPart>, CodecError>> {
        Box::pin(async move {
            if self.failed.load(Ordering::Acquire) {
                return Err(failure(
                    CodecErrorKind::Multipart,
                    "multipart decoder terminated",
                ));
            }
            if self.done {
                return Ok(None);
            }
            let driver = self.driver.clone();
            let mut future = Box::pin(self.inner.next_field());
            let result = futures_util::future::poll_fn(|cx| {
                for _ in 0..32 {
                    if let Some(e) = access(&driver, |p| p.error.take()) {
                        return Poll::Ready(Err(e));
                    }
                    match future.as_mut().poll(cx) {
                        Poll::Ready(Ok(v)) => return Poll::Ready(Ok(v)),
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(map_error(e))),
                        Poll::Pending => {
                            if access(&driver, |p| p.poll_feed(cx)).is_pending() {
                                return Poll::Pending;
                            }
                        }
                    }
                }
                cx.waker().wake_by_ref();
                Poll::Pending
            })
            .await;
            let field = match result {
                Ok(v) => v,
                Err(e) => {
                    let concurrent = e.source_error().is_some_and(|s| {
                        s.downcast_ref::<multer::Error>()
                            .is_some_and(|e| matches!(e, multer::Error::LockFailure))
                    });
                    if !concurrent {
                        self.failed.store(true, Ordering::Release);
                        access(&self.driver, |p| p.close());
                    }
                    return Err(e);
                }
            };
            let Some(field) = field else {
                let result =
                    futures_util::future::poll_fn(|cx| access(&self.driver, |p| p.poll_drain(cx)))
                        .await;
                self.done = true;
                if let Err(e) = result {
                    self.failed.store(true, Ordering::Release);
                    return Err(e);
                }
                return Ok(None);
            };
            if self.parts >= self.limits.max_parts {
                self.failed.store(true, Ordering::Release);
                access(&self.driver, |p| p.close());
                return Err(failure(
                    CodecErrorKind::Limit,
                    "multipart part count exceeds limit",
                ));
            }
            self.parts += 1;
            let headers = field.headers().clone();
            Ok(Some(MultipartPart {
                headers,
                body: HttpBody::Stream(Box::pin(FieldStream {
                    field: Some(field),
                    driver: self.driver.clone(),
                    failed: self.failed.clone(),
                    done: false,
                })),
            }))
        })
    }
}

pub struct MultipartEncoder {
    boundary: String,
    parts: PartStream,
    current: Option<ByteStream>,
    prefix: Option<Bytes>,
    closing_sent: bool,
    failed: bool,
    limits: CodecLimits,
    part_bytes: u64,
    wire_bytes: u64,
    count: usize,
}

impl MultipartEncoder {
    pub fn new(
        boundary: impl Into<String>,
        parts: Vec<MultipartPart>,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        if parts.len() > limits.max_parts {
            return Err(failure(
                CodecErrorKind::Limit,
                "multipart part count exceeds limit",
            ));
        }
        Self::from_multipart(
            boundary,
            Multipart {
                parts: Box::pin(stream::iter(parts.into_iter().map(Ok))),
            },
            limits,
        )
    }
    pub fn from_multipart(
        boundary: impl Into<String>,
        body: Multipart,
        limits: CodecLimits,
    ) -> Result<Self, CodecError> {
        let boundary = boundary.into();

        Ok(Self {
            boundary,
            parts: body.parts,
            current: None,
            prefix: None,
            closing_sent: false,
            failed: false,
            limits,
            part_bytes: 0,
            wire_bytes: 0,
            count: 0,
        })
    }
    fn account(&mut self, n: usize) -> Result<(), CodecError> {
        let next = self.wire_bytes.saturating_add(n as u64);
        if next > self.limits.max_body_bytes {
            return Err(failure(
                CodecErrorKind::Limit,
                "multipart encoded body exceeds limit",
            ));
        }
        self.wire_bytes = next;
        Ok(())
    }
    pub fn next_chunk(&mut self) -> CodecFuture<'_, Result<Option<Bytes>, CodecError>> {
        Box::pin(async move {
            if self.failed {
                return Err(failure(
                    CodecErrorKind::Multipart,
                    "multipart encoder terminated",
                ));
            }
            let result = self.next_inner().await;
            if result.is_err() {
                self.failed = true;
                self.current = None;
                self.prefix = None;
                self.parts = Box::pin(stream::empty());
            }
            result
        })
    }
    async fn next_inner(&mut self) -> Result<Option<Bytes>, CodecError> {
        loop {
            if let Some(prefix) = self.prefix.take() {
                self.account(prefix.len())?;
                return Ok(Some(prefix));
            }
            if let Some(body) = self.current.as_mut() {
                match body.next().await {
                    Some(Ok(chunk)) => {
                        let n = self.part_bytes.saturating_add(chunk.len() as u64);
                        if n > self.limits.max_part_bytes {
                            return Err(failure(
                                CodecErrorKind::Limit,
                                "multipart encoded part exceeds limit",
                            ));
                        }
                        self.part_bytes = n;
                        self.account(chunk.len())?;
                        return Ok(Some(chunk));
                    }
                    Some(Err(e)) => {
                        return Err(CodecError::with_source(
                            CodecErrorKind::Transport,
                            CodecErrorStage::Body,
                            "multipart part stream failed",
                            e,
                        ));
                    }
                    None => {
                        self.current = None;
                        self.prefix = Some(Bytes::from_static(b"\r\n"));
                        continue;
                    }
                }
            }
            if self.closing_sent {
                return Ok(None);
            }
            if let Some(part) = self.parts.next().await {
                let part = part.map_err(|e| {
                    CodecError::with_source(
                        CodecErrorKind::Transport,
                        CodecErrorStage::Body,
                        "multipart parts stream failed",
                        e,
                    )
                })?;
                if self.count >= self.limits.max_parts {
                    return Err(failure(
                        CodecErrorKind::Limit,
                        "multipart part count exceeds limit",
                    ));
                }
                self.count += 1;
                self.part_bytes = 0;
                self.prefix = Some(encode_prefix(&self.boundary, &part, self.limits)?);
                self.current = Some(match part.body {
                    HttpBody::Bytes(b) => Box::pin(stream::once(async move { Ok(b) })),
                    HttpBody::Stream(s) => s,
                });
                continue;
            }
            self.closing_sent = true;
            let end = Bytes::from(format!("--{}--\r\n", self.boundary));
            self.account(end.len())?;
            return Ok(Some(end));
        }
    }
}

fn encode_prefix(
    boundary: &str,
    part: &MultipartPart,
    limits: CodecLimits,
) -> Result<Bytes, CodecError> {
    let mut length = boundary.len().saturating_add(4).saturating_add(2);
    for (name, value) in &part.headers {
        let n = name
            .as_str()
            .len()
            .saturating_add(value.as_bytes().len())
            .saturating_add(2);
        if n as u64 > limits.max_line_bytes {
            return Err(failure(
                CodecErrorKind::Limit,
                "multipart header line exceeds limit",
            ));
        }
        length = length.saturating_add(n).saturating_add(2);
        if length as u64 > limits.max_buffer_bytes {
            return Err(failure(
                CodecErrorKind::Limit,
                "multipart headers exceed buffer limit",
            ));
        }
    }
    if length as u64 > limits.max_buffer_bytes {
        return Err(failure(
            CodecErrorKind::Limit,
            "multipart prefix exceeds buffer limit",
        ));
    }
    let mut output = Vec::with_capacity(length);
    output.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    for (name, value) in &part.headers {
        output.extend_from_slice(name.as_str().as_bytes());
        output.extend_from_slice(b": ");
        output.extend_from_slice(value.as_bytes());
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(b"\r\n");
    Ok(Bytes::from(output))
}
