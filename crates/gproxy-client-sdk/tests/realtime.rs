#![cfg(feature = "realtime")]
use gproxy_client_sdk::{realtime::*, *};
use protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityFuture, UpstreamConnection},
    connection::{Bytes, WebSocket, WsFrame},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
fn codec() -> OpenaiSessionCodec {
    OpenaiSessionCodec::new(&OpenaiRealtimeOptions::new("audio-model").session)
}
#[test]
fn commands_are_native_typed_and_input_audio_transcript_is_collected() {
    let mut codec = codec();
    let audio = codec.command(Command::Audio { bytes: vec![1, 2] }).unwrap();
    assert_eq!(
        serde_json::to_value(audio).unwrap(),
        json!({"type":"input_audio_buffer.append","audio":"AQI="})
    );
    codec.command(Command::CommitAudio).unwrap();
    codec.receive(&json!({"type":"input_audio_buffer.committed","event_id":"e1","item_id":"user_audio","previous_item_id":null}).to_string()).unwrap();
    codec.receive(&json!({"type":"conversation.item.input_audio_transcription.completed","event_id":"e2","item_id":"user_audio","content_index":0,"transcript":"hello","usage":{"type":"duration","seconds":1.0}}).to_string()).unwrap();
    let messages = codec.conversation().into_messages();
    let Content::Audio(audio) = &messages[0].content[0].content else {
        panic!()
    };
    assert_eq!(audio.source, Asset::Inline { bytes: vec![1, 2] });
    assert_eq!(audio.transcript.as_deref(), Some("hello"));
    let text = codec
        .command(Command::Text {
            text: "next".into(),
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(text).unwrap()["item"]["content"][0],
        json!({"type":"input_text","text":"next"})
    );
    let result = codec
        .command(Command::ToolResult(ToolResult {
            id: "call_1".into(),
            name: "weather".into(),
            output: ToolOutput::Json(json!({"temperature":20})),
            is_error: false,
            provider_executed: false,
        }))
        .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap()["item"]["type"],
        "function_call_output"
    );
    assert_eq!(codec.conversation().into_messages()[2].role, Role::Tool);
}
#[test]
fn output_audio_text_and_tool_events_become_portable_ledger_blocks() {
    let mut codec = codec();
    let base = json!({"type":"response.output_audio.delta","event_id":"e1","response_id":"r1","item_id":"m1","output_index":0,"content_index":0,"delta":"AQID"});
    assert!(
        matches!(codec.receive(&base.to_string()).unwrap(),RealtimeEvent::AudioDelta {bytes,..} if bytes==vec![1,2,3])
    );
    let transcript = json!({"type":"response.output_audio_transcript.delta","event_id":"e2","response_id":"r1","item_id":"m1","output_index":0,"content_index":0,"delta":"hi"});
    assert!(
        matches!(codec.receive(&transcript.to_string()).unwrap(),RealtimeEvent::TranscriptDelta {delta,..} if delta=="hi")
    );
    let done = json!({"type":"response.output_item.done","event_id":"e3","response_id":"r1","output_index":0,"item":{"type":"message","role":"assistant","id":"m1","content":[{"type":"output_audio","transcript":"hi","future":1}]}});
    codec.receive(&done.to_string()).unwrap();
    codec.receive(&done.to_string()).unwrap();
    let call = json!({"type":"response.output_item.done","event_id":"e4","response_id":"r1","output_index":1,"item":{"type":"function_call","id":"fc1","call_id":"call_1","name":"weather","arguments":"{ \"city\": \"Shanghai\" }"}});
    codec.receive(&call.to_string()).unwrap();
    // Both item.done surfaces can report the same function item; retain one.
    codec.receive(&call.to_string()).unwrap();
    let messages = codec.conversation().into_messages();
    assert_eq!(messages.len(), 2);
    let Content::Audio(audio) = &messages[0].content[0].content else {
        panic!()
    };
    assert_eq!(
        audio.source,
        Asset::Inline {
            bytes: vec![1, 2, 3]
        }
    );
    assert_eq!(audio.transcript.as_deref(), Some("hi"));
    assert!(
        matches!(&messages[1].content[0].content,Content::ToolCall(ToolCall {id,provider_executed:false,..}) if id=="call_1")
    );
    assert!(matches!(
        codec.receive("{invalid"),
        Err(Error::InvalidData { .. })
    ));
}
struct Fixture {
    sent: Arc<Mutex<Vec<WsFrame>>>,
    seen: Mutex<Vec<(OperationKey, String, String)>>,
    body: Mutex<Vec<u8>>,
}
impl Fixture {
    fn new() -> Self {
        Self {
            sent: Arc::new(Mutex::new(Vec::new())),
            seen: Mutex::new(Vec::new()),
            body: Mutex::new(Vec::new()),
        }
    }
}
impl Backend for Fixture {
    fn connect(
        &self,
        key: OperationKey,
        model: &str,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<UpstreamConnection>> {
        self.seen
            .lock()
            .unwrap()
            .push((key, model.into(), request.query.unwrap()));
        let sent = self.sent.clone();
        Box::pin(async move {
            let outgoing = futures_util::sink::unfold(sent, |sent, frame| async move {
                sent.lock().unwrap().push(frame);
                Ok::<_, protocol::connection::TransportError>(sent)
            });
            let incoming=futures_util::stream::iter(vec![Ok(WsFrame::Text(json!({"type":"response.output_text.delta","event_id":"e1","response_id":"r1","item_id":"m1","output_index":0,"content_index":0,"delta":"hi"}).to_string())),Ok(WsFrame::Close(None))]);
            Ok(UpstreamConnection::Connected {
                handshake: WireResponse {
                    status: http::StatusCode::SWITCHING_PROTOCOLS,
                    headers: http::HeaderMap::new(),
                    body: (),
                },
                socket: WebSocket {
                    incoming: Box::pin(incoming),
                    outgoing: Box::pin(outgoing),
                },
            })
        })
    }
    fn send(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.unwrap().to_owned();
        Box::pin(async move {
            self.seen.lock().unwrap().push((key, model, request.path));
            *self.body.lock().unwrap() =
                protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                    .await
                    .unwrap()
                    .to_vec();
            let mut headers = http::HeaderMap::new();
            headers.insert(
                http::header::LOCATION,
                http::HeaderValue::from_static("/v1/realtime/calls/call_1"),
            );
            Ok(WireResponse {
                status: http::StatusCode::CREATED,
                headers,
                body: HttpBody::Bytes(Bytes::from_static(b"v=0\r\nanswer")),
            })
        })
    }
}
#[test]
fn session_uses_connect_realtime_and_webrtc_uses_create_realtime_call() {
    futures_executor::block_on(async {
        let client = Client::new(Fixture::new());
        let mut session = client
            .connect_realtime(OpenaiRealtimeOptions::new("provider/audio model"))
            .await
            .unwrap();
        session
            .send(Command::Text {
                text: "hello".into(),
            })
            .await
            .unwrap();
        assert!(
            matches!(session.receive().await.unwrap(),Some(RealtimeEvent::TextDelta {delta,..}) if delta=="hi")
        );
        assert!(session.receive().await.unwrap().is_none());
        assert_eq!(
            session.conversation().into_messages()[0].content[0],
            Block::text("hello")
        );
        let sent = client.backend.sent.lock().unwrap();
        let WsFrame::Text(settings) = &sent[0] else {
            panic!()
        };
        assert_eq!(
            serde_json::from_str::<Value>(settings).unwrap()["type"],
            "session.update"
        );
        drop(sent);
        let call = client
            .create_realtime_call(
                OpenaiRealtimeOptions::new("provider/audio model"),
                "v=0\r\noffer".into(),
            )
            .await
            .unwrap();
        assert_eq!(call.sdp, "v=0\r\nanswer");
        assert_eq!(call.location.as_deref(), Some("/v1/realtime/calls/call_1"));
        let seen = client.backend.seen.lock().unwrap();
        assert_eq!(seen[0].0.operation, protocol::Operation::ConnectRealtime);
        assert!(seen[0].2.contains("provider%2Faudio%20model"));
        assert_eq!(seen[1].0.operation, protocol::Operation::CreateRealtimeCall);
        let body = client.backend.body.lock().unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("name=\"sdp\""));
        assert!(body.contains("\"model\":\"provider/audio model\""));
    });
}

#[test]
fn split_session_can_send_while_receive_is_pending() {
    futures_executor::block_on(async {
        let client = Client::new(Fixture::new());
        let session = client
            .connect_realtime(OpenaiRealtimeOptions::new("audio-model"))
            .await
            .unwrap();
        let (mut sender, mut receiver) = session.split();
        let (sent, received) = futures_util::join!(
            sender.send(Command::Text {
                text: "parallel".into()
            }),
            receiver.receive()
        );
        sent.unwrap();
        assert!(matches!(
            received.unwrap(),
            Some(RealtimeEvent::TextDelta { .. })
        ));
        assert_eq!(
            sender.conversation().unwrap(),
            receiver.conversation().unwrap()
        );
        assert_eq!(
            sender.conversation().unwrap().into_messages()[0].content[0],
            Block::text("parallel")
        );
    });
}
