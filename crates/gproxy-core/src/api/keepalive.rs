//! SDK-compatible heartbeats while a buffered upstream result is pending.
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
        let heartbeat = match dialect {
            Dialect::OpenAi | Dialect::OpenAiChat => SSE_COMMENT,
            Dialect::Claude => CLAUDE_PING,
            Dialect::Gemini => b"\n\n",
            _ => return pending.await,
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
                                            gproxy_channel::ChannelError::InvalidResponse(
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

fn byte_stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async { Ok(bytes) })),
        HttpBody::Stream(stream) => stream,
    }
}

fn error_event(dialect: Dialect, status: u16, message: &str) -> Bytes {
    let upstream = serde_json::from_str::<serde_json::Value>(message).ok();
    let detail = upstream
        .as_ref()
        .and_then(|v| v.pointer("/error/message"))
        .and_then(|v| v.as_str())
        .unwrap_or(message);
    let value = match dialect {
        Dialect::OpenAi => {
            serde_json::json!({"type":"error", "code":"upstream_error", "message":detail, "param":null, "status":status})
        }
        Dialect::Claude => {
            serde_json::json!({"type":"error", "error":{"type":"api_error", "message":detail}, "status":status})
        }
        _ => serde_json::json!({"error":{"type":"api_error", "message":detail, "code":status}}),
    };
    Bytes::from(format!("event: error\ndata: {value}\n\n"))
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
                crate::rt::sleep(Duration::from_secs(40)).await;
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
                crate::rt::sleep(Duration::from_secs(20)).await;
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
        assert!(text.contains("over quota") && text.contains("429"));
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
