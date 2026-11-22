//! SDK-compatible heartbeats while a buffered upstream result is pending, and
//! through quiet stretches of an SSE stream that is already running.
//! The pending execution retains its own retries, cancellation and settlement.
use super::{CoreError, HttpExecution, Settled, UsageCompletion};
use futures_util::StreamExt;
use gproxy_protocol::{
    Dialect, HttpBody, WireResponse,
    capability::CapabilityFuture,
    connection::{ByteStream, Bytes},
};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use std::{fmt::Display, time::Duration};
use tokio::sync::oneshot;

const INTERVAL: Duration = Duration::from_secs(15);
const SSE_COMMENT: &[u8] = b":\n\n";
const CLAUDE_PING: &[u8] = b"event: ping\ndata: {\"type\":\"ping\"}\n\n";

type Pending<E> = CapabilityFuture<'static, Result<HttpExecution, E>>;

enum State<E> {
    Waiting(Pending<E>, oneshot::Sender<UsageCompletion>),
    Body(ByteStream),
}

impl HttpExecution {
    /// Keep an SSE client connected while a complete upstream result is pending.
    /// Call only for an SSE generation request mapped to buffered generation.
    /// Fast responses/errors retain their HTTP status and headers. After the
    /// first heartbeat commits HTTP 200, a late failure is an SSE error event.
    /// Dropping the body drops the pending execution, preserving cancellation.
    pub async fn with_sse_keepalive<E: Display + Send + 'static>(
        mut pending: Pending<E>,
        dialect: Dialect,
    ) -> Result<Self, E> {
        let Some(heartbeat) = heartbeat(dialect) else {
            return pending.await;
        };
        if let Some(result) = crate::rt::timeout(INTERVAL, &mut pending).await {
            return result;
        }
        let (send_usage, receive_usage) = oneshot::channel::<UsageCompletion>();
        let waiting = futures_util::stream::unfold(
            Some(State::Waiting(pending, send_usage)),
            move |state| async move {
                let mut state = state?;
                loop {
                    match state {
                        State::Waiting(mut pending, send_usage) => {
                            let Some(result) = crate::rt::timeout(INTERVAL, &mut pending).await
                            else {
                                return Some((
                                    Ok(Bytes::from_static(heartbeat)),
                                    Some(State::Waiting(pending, send_usage)),
                                ));
                            };
                            match result {
                                Ok(execution) => {
                                    let (response, usage) = execution.into_parts();
                                    let _ = send_usage.send(usage);
                                    if !response.status.is_success() {
                                        let status = response.status;
                                        let mut bytes = Vec::new();
                                        let mut body = byte_stream(response.body);
                                        while let Some(chunk) = body.next().await {
                                            match chunk {
                                                Ok(chunk) => bytes.extend_from_slice(&chunk),
                                                Err(error) => return Some((Err(error), None)),
                                            }
                                        }
                                        return Some((
                                            Ok(error_event(
                                                dialect,
                                                status.as_u16(),
                                                &String::from_utf8_lossy(&bytes),
                                            )),
                                            None,
                                        ));
                                    }
                                    state = State::Body(byte_stream(response.body));
                                }
                                Err(error) => {
                                    let message = error.to_string();
                                    let report_error = message.clone();
                                    let _ = send_usage.send(Box::pin(async move {
                                        Err(CoreError::Channel(
                                            gproxy_upstream::UpstreamError::InvalidResponse(
                                                report_error,
                                            ),
                                        ))
                                    }));
                                    return Some((Ok(error_event(dialect, 502, &message)), None));
                                }
                            }
                        }
                        State::Body(mut body) => {
                            return body
                                .next()
                                .await
                                .map(|chunk| (chunk, Some(State::Body(body))));
                        }
                    }
                }
            },
        );
        let body = futures_util::stream::once(async move { Ok(Bytes::from_static(heartbeat)) })
            .chain(waiting);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
        Ok(Self::new(
            WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Stream(Box::pin(body)),
            },
            Box::pin(async move { receive_usage.await.map_err(|_| CoreError::Cancelled)?.await }),
            Settled(()),
        ))
    }
}

impl HttpExecution {
    /// Keep an SSE client connected through quiet stretches of a running
    /// stream, such as a model thinking between two events. Only a successful
    /// `text/event-stream` body is touched. A heartbeat is sent after
    /// [`INTERVAL`] without bytes, and only between two events, so an event
    /// the upstream split across chunks is never cut. Heartbeats are added
    /// outside the observed body: usage, capture and the upstream idle timeout
    /// do not see them.
    pub fn with_sse_idle_heartbeat(self, dialect: Dialect) -> Self {
        let Some(heartbeat) = heartbeat(dialect) else {
            return self;
        };
        let response = self.response();
        let sse = response.status.is_success()
            && !response.headers.contains_key(header::CONTENT_ENCODING)
            && response
                .headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("text/event-stream")
                });
        if !sse {
            return self;
        }
        self.map_response(|mut response| {
            response.body = match response.body {
                HttpBody::Stream(inner) => HttpBody::Stream(idle_heartbeats(inner, heartbeat)),
                bytes => bytes,
            };
            response
        })
    }
}

fn heartbeat(dialect: Dialect) -> Option<&'static [u8]> {
    match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::GeminiInteractions => Some(SSE_COMMENT),
        Dialect::Claude => Some(CLAUDE_PING),
        Dialect::Gemini => Some(b"\n\n"),
        _ => None,
    }
}

/// The last bytes sent, enough to tell whether the stream stands between two
/// SSE events: nothing sent yet, or a blank line last.
#[derive(Default)]
struct Tail(Vec<u8>);

impl Tail {
    fn push(&mut self, chunk: &[u8]) {
        self.0
            .extend_from_slice(&chunk[chunk.len().saturating_sub(4)..]);
        let extra = self.0.len().saturating_sub(4);
        self.0.drain(..extra);
    }

    fn between_events(&self) -> bool {
        let tail = self.0.as_slice();
        if tail.is_empty() {
            return true;
        }
        tail.strip_suffix(b"\r\n")
            .or_else(|| tail.strip_suffix(b"\n"))
            .or_else(|| tail.strip_suffix(b"\r"))
            .is_some_and(|rest| matches!(rest.last(), Some(b'\n' | b'\r')))
    }
}

fn idle_heartbeats(inner: ByteStream, heartbeat: &'static [u8]) -> ByteStream {
    Box::pin(futures_util::stream::unfold(
        (inner, Tail::default()),
        move |(mut inner, mut tail)| async move {
            loop {
                match crate::rt::timeout(INTERVAL, inner.next()).await {
                    Some(Some(chunk)) => {
                        if let Ok(bytes) = &chunk {
                            tail.push(bytes);
                        }
                        return Some((chunk, (inner, tail)));
                    }
                    Some(None) => return None,
                    None if tail.between_events() => {
                        return Some((Ok(Bytes::from_static(heartbeat)), (inner, tail)));
                    }
                    // Mid-event: wait for the upstream to finish it.
                    None => {}
                }
            }
        },
    ))
}

fn byte_stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async { Ok(bytes) })),
        HttpBody::Stream(stream) => stream,
    }
}

fn error_event(dialect: Dialect, status: u16, message: &str) -> Bytes {
    let detail = serde_json::from_str::<serde_json::Value>(message)
        .ok()
        .map(crate::convert::upstream_error::unwrap)
        .filter(|detail| detail["message"].is_string())
        .unwrap_or_else(|| serde_json::json!({"message": message}));
    let value = crate::convert::upstream_error::event(&detail, status, dialect, 0)
        .expect("SSE generation dialect");
    if dialect == Dialect::OpenAiChat {
        Bytes::from(format!("data: {value}\n\ndata: [DONE]\n\n"))
    } else {
        Bytes::from(format!("event: error\ndata: {value}\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    fn execution(status: StatusCode, body: &'static [u8]) -> HttpExecution {
        HttpExecution::new(
            WireResponse {
                status,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from_static(body)),
            },
            Box::pin(async { Err(CoreError::Forbidden("original completion")) }),
            Settled(()),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn slow_result_gets_heartbeats_then_original_body_and_completion() {
        let result = HttpExecution::with_sse_keepalive::<CoreError>(
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(40)).await;
                Ok(execution(StatusCode::OK, b"data: answer\n\n"))
            }),
            Dialect::OpenAiChat,
        )
        .await
        .unwrap();
        let (response, usage) = result.into_parts();
        assert_eq!(response.headers[header::CONTENT_TYPE], "text/event-stream");
        let mut body = byte_stream(response.body);
        assert_eq!(body.next().await.unwrap().unwrap(), SSE_COMMENT);
        assert_eq!(body.next().await.unwrap().unwrap(), SSE_COMMENT);
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            b"data: answer\n\n".as_slice()
        );
        assert!(body.next().await.is_none());
        assert!(matches!(
            usage.await,
            Err(CoreError::Forbidden("original completion"))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn quick_rejections_keep_status_and_late_rejections_become_sse_errors() {
        let result = HttpExecution::with_sse_keepalive::<CoreError>(
            Box::pin(async { Ok(execution(StatusCode::BAD_REQUEST, b"rejected")) }),
            Dialect::OpenAi,
        )
        .await
        .unwrap();
        assert_eq!(result.response().status, StatusCode::BAD_REQUEST);
        let late = HttpExecution::with_sse_keepalive::<CoreError>(
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(20)).await;
                Ok(execution(
                    StatusCode::TOO_MANY_REQUESTS,
                    br#"{"error":{"message":"over quota"}}"#,
                ))
            }),
            Dialect::Claude,
        )
        .await
        .unwrap();
        let (response, usage) = late.into_parts();
        let mut body = byte_stream(response.body);
        assert_eq!(body.next().await.unwrap().unwrap(), CLAUDE_PING);
        let event = body.next().await.unwrap().unwrap();
        let text = std::str::from_utf8(&event).unwrap();
        assert!(text.starts_with("event: error\n"));
        assert!(text.contains("over quota") && text.contains("rate_limit_error"));
        assert!(body.next().await.is_none());
        assert!(matches!(
            usage.await,
            Err(CoreError::Forbidden("original completion"))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_heartbeat_body_drops_pending_work() {
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Guard(dropped.clone());
        let result = HttpExecution::with_sse_keepalive::<CoreError>(
            Box::pin(async move {
                let _guard = guard;
                std::future::pending::<()>().await;
                unreachable!()
            }),
            Dialect::OpenAi,
        )
        .await
        .unwrap();
        let (response, usage) = result.into_parts();
        drop(response);
        assert!(dropped.load(Ordering::SeqCst));
        assert!(matches!(usage.await, Err(CoreError::Cancelled)));
    }
}
