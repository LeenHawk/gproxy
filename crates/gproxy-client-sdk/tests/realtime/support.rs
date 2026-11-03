pub use futures_util::StreamExt;
pub use gproxy_client_sdk::{realtime::*, *};
use gproxy_protocol::{
    capability::{CapabilityFuture, UpstreamConnection},
    connection::{TransportError, WebSocket, WsClose, WsFrame},
};
pub use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
pub type Sent = Arc<Mutex<Vec<WsFrame>>>;
#[derive(Clone, Debug)]
pub struct Handshake {
    pub operation: protocol::OperationKey,
    pub model: String,
    pub path: String,
    pub query: Option<String>,
}
pub struct Script {
    pub frames: Mutex<Option<Vec<WsFrame>>>,
    pub sent: Sent,
    pub handshake: Mutex<Option<Handshake>>,
    pub reject: Option<(u16, String)>,
    pub pending: bool,
}
impl Script {
    pub fn new(frames: Vec<Value>) -> Self {
        Self {
            frames: Mutex::new(Some(
                frames
                    .into_iter()
                    .map(|v| WsFrame::Text(v.to_string()))
                    .chain([WsFrame::Close(Some(WsClose {
                        code: 1000,
                        reason: "fixture complete".into(),
                    }))])
                    .collect(),
            )),
            sent: Arc::default(),
            handshake: Mutex::new(None),
            reject: None,
            pending: false,
        }
    }
    pub fn fixture(source: &str) -> Self {
        Self::new(
            source
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| serde_json::from_str(l).unwrap())
                .collect(),
        )
    }
    pub fn values(&self) -> Vec<Value> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter_map(|f| match f {
                WsFrame::Text(t) => Some(serde_json::from_str(t).unwrap()),
                _ => None,
            })
            .collect()
    }
    pub fn socket(&self) -> WebSocket {
        let sent = self.sent.clone();
        let outgoing = futures_util::sink::unfold(sent, |sent, frame| async move {
            sent.lock().unwrap().push(frame);
            Ok::<_, TransportError>(sent)
        });
        let frames = self.frames.lock().unwrap().take().unwrap();
        let incoming: protocol::connection::WsReceiver = if self.pending {
            Box::pin(
                futures_util::stream::iter(
                    frames
                        .into_iter()
                        .filter(|f| !matches!(f, WsFrame::Close(_)))
                        .map(Ok),
                )
                .chain(futures_util::stream::pending()),
            )
        } else {
            Box::pin(futures_util::stream::iter(frames.into_iter().map(Ok)))
        };
        WebSocket {
            incoming,
            outgoing: Box::pin(outgoing),
        }
    }
}
impl Backend for Script {
    fn send(
        &self,
        _: protocol::OperationKey,
        _: Option<&str>,
        _: protocol::WireRequest<protocol::HttpBody>,
    ) -> CapabilityFuture<'_, Result<protocol::WireResponse<protocol::HttpBody>>> {
        Box::pin(async { panic!("Realtime must use Backend::connect") })
    }
    fn connect(
        &self,
        operation: protocol::OperationKey,
        model: &str,
        request: protocol::WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<UpstreamConnection>> {
        *self.handshake.lock().unwrap() = Some(Handshake {
            operation,
            model: model.to_owned(),
            path: request.path,
            query: request.query,
        });
        Box::pin(async move {
            if let Some((status, body)) = &self.reject {
                return Ok(UpstreamConnection::Rejected(protocol::WireResponse {
                    status: http::StatusCode::from_u16(*status).unwrap(),
                    headers: http::HeaderMap::new(),
                    body: protocol::HttpBody::Bytes(body.clone().into_bytes().into()),
                }));
            }
            Ok(UpstreamConnection::Connected {
                handshake: protocol::WireResponse {
                    status: http::StatusCode::SWITCHING_PROTOCOLS,
                    headers: http::HeaderMap::new(),
                    body: (),
                },
                socket: self.socket(),
            })
        })
    }
}
pub fn config(dialect: RealtimeDialect) -> RealtimeConfig {
    RealtimeConfig::new(RealtimeModel {
        id: "provider/model".into(),
        dialect,
    })
}
pub fn ready() -> Value {
    json!({"type":"session.updated","event_id":"s1","session":{"type":"realtime","id":"session-1","object":"realtime.session"}})
}
pub fn output(kind: &str, field: &str, value: Value) -> Value {
    let mut e = json!({"type":kind,"event_id":"e","item_id":"answer","response_id":"response-1","output_index":0,"content_index":0});
    e[field] = value;
    e
}
pub async fn next(
    session: &mut RealtimeSession,
    collector: &mut RealtimeCollector,
) -> RealtimeEvent {
    let event = session.events.next().await.unwrap();
    collector.push(&event);
    event
}
pub fn roundtrip(conversation: Conversation) -> (Vec<Message>, String) {
    let json = serde_json::to_string(&conversation).unwrap();
    assert_eq!(
        serde_json::from_str::<Conversation>(&json).unwrap(),
        conversation
    );
    assert!(!json.contains("audio_delta"));
    let messages = conversation.into_messages();
    assert!(
        messages
            .iter()
            .flat_map(|m| &m.content)
            .all(|b| !matches!(b.content, Content::Audio(_)))
    );
    (messages, json)
}
