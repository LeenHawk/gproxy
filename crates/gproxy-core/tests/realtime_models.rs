#![cfg(not(target_arch = "wasm32"))]
mod support;
use futures_util::{SinkExt, StreamExt};
use gproxy_core::{
    CoreError,
    realtime::connect::{model, setup_model},
};
use gproxy_protocol::{
    Dialect, Operation, OperationKey, WireRequest, capability::UpstreamConnection,
    connection::WsFrame,
};
use support::*;

fn key(dialect: Dialect) -> OperationKey {
    OperationKey {
        operation: Operation::ConnectRealtime,
        dialect,
    }
}
fn wire() -> WireRequest<()> {
    WireRequest {
        method: http::Method::GET,
        path: "/v1/realtime".into(),
        query: None,
        headers: http::HeaderMap::new(),
        body: (),
    }
}
#[test]
fn realtime_selectors_decode_models_and_keep_call_continuations() {
    let op = key(Dialect::OpenAi);
    assert_eq!(
        model(op, "/v1/realtime", Some("model=p%2Falias"), None)
            .unwrap()
            .as_deref(),
        Some("p/alias")
    );
    for query in [
        None,
        Some("model="),
        Some("model=+"),
        Some("model=a&model=b"),
        Some("model=a&%6Dodel=b"),
    ] {
        assert!(model(op, "/v1/realtime", query, None).is_err(), "{query:?}");
    }
    assert_eq!(
        model(op, "/v1/realtime", Some("call_id=rtc_existing"), None).unwrap(),
        None
    );
    assert_eq!(
        model(op, "/v1/live/rtc_existing", None, None).unwrap(),
        None
    );
    assert!(model(op, "/v1/realtime", Some("call_id=a&call_id=b"), None).is_err());
    assert_eq!(
        model(op, "/v1/realtime", None, Some("mounted/alias"))
            .unwrap()
            .as_deref(),
        Some("mounted/alias")
    );
    assert!(model(key(Dialect::Gemini), "/ws/live", None, None).is_err());
}
#[test]
fn gemini_setup_requires_a_model_and_keeps_future_controls_opaque() {
    assert_eq!(
        setup_model(br#"{"setup":{"model":"models/space/alias","future":{"mode":"unknown"}}}"#)
            .unwrap(),
        "space/alias"
    );
    for json in [
        "not-json",
        "{}",
        r#"{"setup":{}}"#,
        r#"{"setup":{"model":3}}"#,
        r#"{"setup":{"model":"models/"}}"#,
        r#"{"setup":{"model":" "}}"#,
    ] {
        assert!(setup_model(json.as_bytes()).is_err(), "{json}");
    }
}
#[tokio::test]
async fn resolved_model_is_bound_into_openai_create_and_gemini_setup_frames() {
    for dialect in [Dialect::OpenAi, Dialect::Gemini] {
        let h = harness(full(), "round_robin").await;
        h.script_ws(vec![WsReply::Connected(Vec::new())]);
        let mut request = wire();
        if dialect == Dialect::Gemini {
            request.query = Some("call_id=unrelated-openai-selector".into());
        }
        let execution = h
            .core
            .connect(h.context_for("p", key(dialect), "model", 1, None), request)
            .await
            .unwrap();
        let (connection, usage) = execution.into_parts();
        let UpstreamConnection::Connected { mut socket, .. } = connection else {
            panic!("connected")
        };
        if dialect == Dialect::OpenAi {
            socket
                .outgoing
                .send(WsFrame::Text(
                    r#"{"type":"response.create","model":"untrusted"}"#.into(),
                ))
                .await
                .unwrap();
            socket
                .outgoing
                .send(WsFrame::Text(
                    r#"{"type":"session.update","session":{"model":"untrusted"}}"#.into(),
                ))
                .await
                .unwrap();
            let frames = h.client.sent_frames.lock().unwrap();
            let WsFrame::Text(text) = &frames[0] else {
                panic!("text")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(text).unwrap()["model"],
                "gpt-x"
            );
            let WsFrame::Text(text) = &frames[1] else {
                panic!("update")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(text).unwrap()["session"]["model"],
                "gpt-x"
            );
        } else {
            for frame in [
                WsFrame::Text(r#"{"setup":{"model":"models/alias","future":3}}"#.into()),
                WsFrame::Binary(
                    r#"{"setup":{"model":"models/another","future":3}}"#.as_bytes().to_vec().into(),
                ),
            ] {
                socket.outgoing.send(frame).await.unwrap();
            }
            let frames = h.client.sent_frames.lock().unwrap();
            for frame in frames.iter() {
                let bytes = match frame {
                    WsFrame::Text(t) => t.as_bytes(),
                    WsFrame::Binary(b) => b.as_ref(),
                    _ => panic!("JSON"),
                };
                let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
                assert_eq!(value["setup"]["model"], "models/gpt-x");
                assert_eq!(value["setup"]["future"], 3);
            }
        }
        while socket.incoming.next().await.is_some() {}
        drop(socket);
        usage.await.unwrap();
    }
}
#[tokio::test]
async fn a_new_realtime_handshake_cannot_open_without_a_resolved_model() {
    for dialect in [Dialect::OpenAi, Dialect::Gemini] {
        let h = harness(full(), "round_robin").await;
        let mut context = (*h.context_for("p", key(dialect), "missing", 1, None)).clone();
        context.target.upstream_model = None;
        assert!(matches!(
            h.core.connect(std::sync::Arc::new(context), wire()).await,
            Err(CoreError::InvalidTarget(_))
        ));
        assert!(h.client.seen.lines().is_empty());
    }
}
