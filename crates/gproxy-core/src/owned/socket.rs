//! File references in client messages on a passthrough socket.
//!
//! A realtime or Live socket is bound to one credential at its handshake, and
//! after that every client message goes straight to the upstream. A message
//! can name an uploaded file — Gemini Live `clientContent` / `realtimeInput`
//! parts with `fileData.fileUri`, a `file_id` in an OpenAI realtime
//! `conversation.item.create` — so each text (or JSON binary) message is
//! walked with the same field rules as an HTTP body (`super::walk_references`)
//! before it is forwarded. Every file it names must be the caller scope's on
//! this provider, on the credential the socket is bound to. A message that
//! fails is **not forwarded**; the client is told instead:
//!
//! * OpenAI realtime: an `error` event (`invalid_request_error`,
//!   `file_not_found`), carrying the client's `event_id`; the session goes on.
//! * Gemini Live, which has no error event, closes with `1008` (policy
//!   violation) and the reason — what the Live API itself does with a
//!   message it refuses.
//!
//! The check needs the store, so it runs inside the sink's readiness: a
//! message with references is held until its lookup finishes and is then
//! forwarded or dropped, in order with every other message.

use super::{OwnedKind, Placement, placement_in};
use futures_util::{Sink, StreamExt};
use gproxy_protocol::{
    Dialect,
    capability::CapabilityFuture,
    connection::{TransportError, WebSocket, WsClose, WsFrame, WsSender},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};
use tokio::sync::mpsc;

/// Looks up where a set of referenced files lives, for this socket's scope
/// and provider.
#[cfg(not(target_arch = "wasm32"))]
type Check = Arc<
    dyn Fn(BTreeSet<String>) -> CapabilityFuture<'static, crate::CoreResult<Placement>>
        + Send
        + Sync,
>;
/// wasm32 is single-threaded and its sockets are not `Send` either.
#[cfg(target_arch = "wasm32")]
type Check =
    Arc<dyn Fn(BTreeSet<String>) -> CapabilityFuture<'static, crate::CoreResult<Placement>>>;

/// The file ids one client message names.
fn references(dialect: Dialect, frame: &WsFrame) -> Option<(BTreeSet<String>, Value)> {
    let value: Value = match frame {
        WsFrame::Text(text) => serde_json::from_str(text).ok()?,
        WsFrame::Binary(bytes) => serde_json::from_slice(bytes).ok()?,
        _ => return None,
    };
    let mut ids = BTreeSet::new();
    super::walk_references(&value, dialect == Dialect::Gemini, &mut ids);
    (!ids.is_empty()).then_some((ids, value))
}

/// What the client is told in place of a refused message.
fn refusal(dialect: Dialect, message: &Value, reason: &str) -> WsFrame {
    if dialect == Dialect::Gemini {
        // A close reason is capped at 123 bytes.
        let mut reason = reason.to_owned();
        while reason.len() > 123 {
            reason.pop();
        }
        return WsFrame::Close(Some(WsClose { code: 1008, reason }));
    }
    WsFrame::Text(
        serde_json::json!({
            "type": "error",
            "event_id": format!("event_gproxy_{}", crate::ids::random_id()),
            "error": {
                "type": "invalid_request_error",
                "code": "file_not_found",
                "message": reason,
                "event_id": message.get("event_id"),
            }
        })
        .to_string(),
    )
}

struct Held {
    frame: WsFrame,
    message: Value,
    lookup: CapabilityFuture<'static, crate::CoreResult<Placement>>,
}

struct GuardedSink {
    inner: WsSender,
    dialect: Dialect,
    credential_id: String,
    check: Check,
    held: Option<Held>,
    /// A checked message waiting for the upstream to be ready.
    ready: Option<WsFrame>,
    refusals: mpsc::UnboundedSender<WsFrame>,
}

impl GuardedSink {
    /// Finish whatever is held: forward it once the upstream is ready, or
    /// send its refusal to the client.
    fn drain(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), TransportError>> {
        loop {
            if let Some(held) = self.held.as_mut() {
                let verdict = ready!(held.lookup.as_mut().poll(cx));
                let held = self.held.take().expect("held");
                let refused = match verdict {
                    Ok(Placement::Anywhere) => None,
                    Ok(Placement::Credential(credential)) if credential == self.credential_id => {
                        None
                    }
                    Ok(Placement::Credential(_)) => Some(
                        "the referenced file lives on another upstream credential than this \
                         connection"
                            .to_owned(),
                    ),
                    Err(crate::CoreError::ResourceNotFound { id, .. }) => {
                        Some(format!("{} `{id}` not found", OwnedKind::File.noun()))
                    }
                    Err(error) => Some(error.to_string()),
                };
                match refused {
                    None => self.ready = Some(held.frame),
                    Some(reason) => {
                        let _ = self
                            .refusals
                            .send(refusal(self.dialect, &held.message, &reason));
                    }
                }
            }
            if self.ready.is_some() {
                ready!(self.inner.as_mut().poll_ready(cx))?;
                let frame = self.ready.take().expect("ready");
                self.inner.as_mut().start_send(frame)?;
            }
            if self.held.is_none() && self.ready.is_none() {
                return Poll::Ready(Ok(()));
            }
        }
    }
}

impl Sink<WsFrame> for GuardedSink {
    type Error = TransportError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.drain(cx))?;
        self.inner.as_mut().poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, frame: WsFrame) -> Result<(), Self::Error> {
        match references(self.dialect, &frame) {
            Some((ids, message)) => {
                let lookup = (self.check)(ids);
                self.held = Some(Held {
                    frame,
                    message,
                    lookup,
                });
                Ok(())
            }
            None => self.inner.as_mut().start_send(frame),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.drain(cx))?;
        self.inner.as_mut().poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.drain(cx))?;
        self.inner.as_mut().poll_close(cx)
    }
}

/// Wrap a passthrough socket bound to `credential_id` so client messages
/// naming files the scope does not own there are refused rather than sent.
pub(crate) fn guard<C: BatchConnectionTrait + Send + Sync + 'static>(
    store: Arc<gproxy_store::Store<C>>,
    scope: String,
    provider_id: String,
    credential_id: String,
    dialect: Dialect,
    socket: WebSocket,
) -> WebSocket {
    let check: Check = Arc::new(move |ids: BTreeSet<String>| {
        let store = store.clone();
        let scope = scope.clone();
        let provider_id = provider_id.clone();
        Box::pin(async move { placement_in(&store, &scope, &provider_id, &ids).await })
    });
    guard_with(check, credential_id, dialect, socket)
}

fn guard_with(
    check: Check,
    credential_id: String,
    dialect: Dialect,
    socket: WebSocket,
) -> WebSocket {
    let (refusals, mut refused) = mpsc::unbounded_channel();
    let mut upstream = socket.incoming;
    let incoming = futures_util::stream::poll_fn(move |cx| {
        if let Poll::Ready(Some(frame)) = refused.poll_recv(cx) {
            return Poll::Ready(Some(Ok(frame)));
        }
        upstream.poll_next_unpin(cx)
    });
    WebSocket {
        incoming: Box::pin(incoming),
        outgoing: Box::pin(GuardedSink {
            inner: socket.outgoing,
            dialect,
            credential_id,
            check,
            held: None,
            ready: None,
            refusals,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;
    use std::sync::Mutex;

    /// An upstream that records what it was sent and never answers.
    fn upstream() -> (WebSocket, Arc<Mutex<Vec<WsFrame>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let record = sent.clone();
        let outgoing = futures_util::sink::unfold((), move |(), frame: WsFrame| {
            record.lock().unwrap().push(frame);
            async { Ok::<_, TransportError>(()) }
        });
        (
            WebSocket {
                incoming: Box::pin(futures_util::stream::pending()),
                outgoing: Box::pin(outgoing),
            },
            sent,
        )
    }

    /// `file-mine` lives on `c1`, `file-other` on `c2`; nothing else exists.
    fn check() -> Check {
        Arc::new(|ids: BTreeSet<String>| {
            Box::pin(async move {
                let mut credentials = BTreeSet::new();
                for id in &ids {
                    match id.as_str() {
                        "file-mine" => credentials.insert("c1"),
                        "file-other" => credentials.insert("c2"),
                        _ => {
                            return Err(crate::CoreError::ResourceNotFound {
                                kind: "file",
                                id: id.clone(),
                            });
                        }
                    };
                }
                Ok(match credentials.into_iter().next() {
                    Some(credential) => Placement::Credential(credential.into()),
                    None => Placement::Anywhere,
                })
            })
        })
    }

    #[tokio::test]
    async fn realtime_messages_naming_foreign_files_are_answered_not_forwarded() {
        let (socket, sent) = upstream();
        let mut socket = guard_with(check(), "c1".into(), Dialect::OpenAi, socket);
        let item = |id: &str| {
            WsFrame::Text(
                serde_json::json!({"type": "conversation.item.create", "event_id": "ev1",
                "item": {"type": "message", "role": "user", "content": [
                    {"type": "input_file", "file_id": id}
                ]}})
                .to_string(),
            )
        };
        socket.outgoing.send(item("file-mine")).await.unwrap();
        socket.outgoing.send(item("file-nope")).await.unwrap();
        socket.outgoing.send(item("file-other")).await.unwrap();
        let plain = WsFrame::Text(r#"{"type":"response.create"}"#.into());
        socket.outgoing.send(plain.clone()).await.unwrap();

        let sent = sent.lock().unwrap().clone();
        assert_eq!(sent, vec![item("file-mine"), plain]);
        for expected in ["`file-nope` not found", "another upstream credential"] {
            let Some(Ok(WsFrame::Text(text))) = socket.incoming.next().await else {
                panic!("an error event")
            };
            let event: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(event["type"], "error");
            assert_eq!(event["error"]["code"], "file_not_found");
            assert_eq!(event["error"]["event_id"], "ev1");
            assert!(
                event["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(expected),
                "{event}"
            );
        }
    }

    #[tokio::test]
    async fn a_live_message_naming_a_foreign_file_closes_with_a_policy_violation() {
        let (socket, sent) = upstream();
        let mut socket = guard_with(check(), "c1".into(), Dialect::Gemini, socket);
        let message = WsFrame::Text(
            serde_json::json!({"clientContent": {"turns": [{"role": "user", "parts": [
                {"fileData": {"fileUri": "https://generativelanguage.googleapis.com/v1beta/files/file-nope"}}
            ]}]}})
            .to_string(),
        );
        socket.outgoing.send(message).await.unwrap();
        assert!(sent.lock().unwrap().is_empty());
        let Some(Ok(WsFrame::Close(Some(close)))) = socket.incoming.next().await else {
            panic!("a close frame")
        };
        assert_eq!(close.code, 1008);
        assert!(close.reason.contains("file-nope"), "{}", close.reason);
    }
}
