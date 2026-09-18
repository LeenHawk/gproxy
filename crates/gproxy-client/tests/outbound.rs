#![cfg(all(not(target_arch = "wasm32"), feature = "reqwest"))]

use futures_util::{SinkExt, StreamExt};
use gproxy_client::{ClientPool, ConnectionConfig, OutboundClient};
use gproxy_protocol::{
    HttpBody,
    capability::UpstreamConnection,
    connection::{WsClose, WsFrame},
};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};

async fn read_head(stream: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        request.push(stream.read_u8().await.unwrap());
    }
    String::from_utf8(request).unwrap()
}

#[tokio::test]
async fn send_streams_chunks_before_the_body_finishes_and_keeps_non_2xx() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let release = Arc::new(Notify::new());
    let gate = release.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let head = read_head(&mut stream).await;
        assert!(head.starts_with("POST /v1/x HTTP/1.1"));
        assert!(head.contains("authorization: Bearer k"));
        let mut body = [0u8; 4];
        stream.read_exact(&mut body).await.unwrap();
        assert_eq!(&body, b"ping");
        stream
            .write_all(
                b"HTTP/1.1 429 Too Many Requests\r\nretry-after: 7\r\ntransfer-encoding: chunked\r\n\r\n5\r\nfirst\r\n",
            )
            .await
            .unwrap();
        stream.flush().await.unwrap();
        gate.notified().await;
        stream.write_all(b"6\r\nsecond\r\n0\r\n\r\n").await.unwrap();
        stream.flush().await.unwrap();
    });
    let client = ClientPool::default()
        .get(&ConnectionConfig::default())
        .await
        .unwrap();
    let request = http::Request::post(format!("http://{address}/v1/x"))
        .header("authorization", "Bearer k")
        .body(HttpBody::Bytes("ping".into()))
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), client.send(request))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status, 429);
    assert_eq!(response.headers["retry-after"], "7");
    let HttpBody::Stream(mut body) = response.body else {
        panic!("native responses stream");
    };
    let first = body.next().await.unwrap().unwrap();
    assert_eq!(first.as_ref(), b"first");
    release.notify_one();
    let second = body.next().await.unwrap().unwrap();
    assert_eq!(second.as_ref(), b"second");
    assert!(body.next().await.is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn connect_keeps_a_rejected_handshake_and_bridges_frames_when_accepted() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let head = read_head(&mut stream).await;
        assert!(head.contains("sec-websocket-key:"));
        assert!(head.contains("sec-websocket-protocol: gproxy-test"));
        assert_eq!(head.matches("upgrade:").count(), 1);
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 6\r\n\r\ndenied")
            .await
            .unwrap();
        stream.flush().await.unwrap();
        drop(stream);
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let message = ws.next().await.unwrap().unwrap();
        assert!(message.is_text());
        ws.send(message).await.unwrap();
        assert!(ws.next().await.unwrap().unwrap().is_close());
    });
    let client = ClientPool::default()
        .get_websocket(&ConnectionConfig::default())
        .await
        .unwrap();
    let request = || {
        http::Request::get(format!("ws://{address}/realtime"))
            .header("upgrade", "websocket")
            .header("sec-websocket-protocol", "gproxy-test")
            .header("authorization", "Bearer k")
            .body(())
            .unwrap()
    };
    let rejected = client.connect(request()).await.unwrap();
    let UpstreamConnection::Rejected(response) = rejected else {
        panic!("handshake was refused");
    };
    assert_eq!(response.status, 403);
    let HttpBody::Stream(mut body) = response.body else {
        panic!("native responses stream");
    };
    assert_eq!(body.next().await.unwrap().unwrap().as_ref(), b"denied");

    let request = http::Request::get(format!("ws://{address}/realtime"))
        .body(())
        .unwrap();
    let UpstreamConnection::Connected { handshake, socket } =
        client.connect(request).await.unwrap()
    else {
        panic!("handshake was accepted");
    };
    assert_eq!(handshake.status, 101);
    let mut outgoing = socket.outgoing;
    let mut incoming = socket.incoming;
    outgoing.send(WsFrame::Text("hello".into())).await.unwrap();
    assert_eq!(
        incoming.next().await.unwrap().unwrap(),
        WsFrame::Text("hello".into())
    );
    outgoing
        .send(WsFrame::Close(Some(WsClose {
            code: 1000,
            reason: String::new(),
        })))
        .await
        .unwrap();
    server.await.unwrap();
}
