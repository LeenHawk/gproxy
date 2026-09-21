#![cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "reqwest", feature = "wreq")
))]

use std::{sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt, stream};
use gproxy_client::{Backend, Client, ClientPool, ConnectionConfig, ProxyConfig};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

fn backends() -> Vec<Backend> {
    vec![
        #[cfg(feature = "reqwest")]
        Backend::Reqwest,
        #[cfg(feature = "wreq")]
        Backend::Wreq,
    ]
}

async fn read_until(stream: &mut TcpStream, suffix: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    while !result.ends_with(suffix) {
        result.push(stream.read_u8().await.unwrap());
    }
    result
}

async fn read_body(stream: &mut TcpStream, headers: &str) -> Vec<u8> {
    if headers.contains("transfer-encoding: chunked") {
        let mut body = Vec::new();
        loop {
            let line = read_until(stream, b"\r\n").await;
            let size =
                usize::from_str_radix(std::str::from_utf8(&line).unwrap().trim(), 16).unwrap();
            if size == 0 {
                assert_eq!(read_until(stream, b"\r\n").await, b"\r\n");
                return body;
            }
            let offset = body.len();
            body.resize(offset + size, 0);
            stream.read_exact(&mut body[offset..]).await.unwrap();
            assert_eq!(read_until(stream, b"\r\n").await, b"\r\n");
        }
    }
    let length = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    body
}

#[tokio::test]
async fn multipart_stream_uploads_work_through_the_configured_proxy() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut stream, _) = proxy.accept().await.unwrap();
                let headers =
                    String::from_utf8(read_until(&mut stream, b"\r\n\r\n").await).unwrap();
                assert!(headers.starts_with("POST http://upload.invalid/files HTTP/1.1\r\n"));
                let lower = headers.to_lowercase();
                let body = read_body(&mut stream, &lower).await;
                let boundary = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(": ")
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                            .and_then(|(_, value)| {
                                value.strip_prefix("multipart/form-data; boundary=")
                            })
                    })
                    .unwrap();
                let body = String::from_utf8(body).unwrap();
                assert!(body.starts_with(&format!("--{boundary}\r\n")));
                assert!(body.ends_with(&format!("--{boundary}--\r\n")));
                assert!(body.contains("name=\"purpose\"\r\n\r\ntest\r\n"));
                assert!(body.contains("name=\"file\"; filename=\"sample.bin\""));
                assert!(body.contains("first-second"));
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await
                    .unwrap();
            });
            let pool = ClientPool::default();
            let client = pool
                .get(&ConnectionConfig {
                    backend,
                    proxy: ProxyConfig::Explicit { url: proxy_url },
                    ..Default::default()
                })
                .await
                .unwrap();
            let chunks =
                stream::iter([Ok::<_, std::io::Error>(&b"first-"[..]), Ok(&b"second"[..])]);
            match client.as_ref() {
                #[cfg(feature = "reqwest-native")]
                Client::ReqwestNative(_) => unreachable!("not exercised here"),
                Client::Host(_) => unreachable!("this pool builds its own clients"),
                #[cfg(feature = "reqwest")]
                Client::Reqwest(client) => {
                    use gproxy_client::reqwest::{
                        Body,
                        multipart::{Form, Part},
                    };
                    let part = Part::stream(Body::wrap_stream(chunks)).file_name("sample.bin");
                    let form = Form::new().text("purpose", "test").part("file", part);
                    let response = client
                        .post("http://upload.invalid/files")
                        .multipart(form)
                        .send()
                        .await
                        .unwrap();
                    assert_eq!(response.bytes().await.unwrap().as_ref(), b"ok");
                }
                #[cfg(feature = "wreq")]
                Client::Wreq(client) => {
                    use gproxy_client::wreq::{
                        Body,
                        multipart::{Form, Part},
                    };
                    let part = Part::stream(Body::wrap_stream(chunks)).file_name("sample.bin");
                    let form = Form::new().text("purpose", "test").part("file", part);
                    let response = client
                        .post("http://upload.invalid/files")
                        .multipart(form)
                        .send()
                        .await
                        .unwrap();
                    assert_eq!(response.bytes().await.unwrap().as_ref(), b"ok");
                }
            }
            server.await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn websocket_upgrade_and_duplex_frames_use_the_profile_and_survive_cache_clear() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            for proxied in [false, true] {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move {
                    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
                    let (stream, _) = listener.accept().await.unwrap();
                    // Tungstenite fixes the callback error type to an HTTP response.
                    #[allow(clippy::result_large_err)]
                    let mut ws = tokio_tungstenite::accept_hdr_async(stream, move |request: &Request, mut response: Response| {
                        assert_eq!(request.uri().path(), "/chat");
                        assert_eq!(request.headers()["authorization"], "Bearer upstream-token");
                        assert_eq!(request.headers()["sec-websocket-protocol"], "gproxy-test");
                        if proxied {
                            assert_eq!(request.uri().host(), Some("origin.invalid"));
                            assert_eq!(request.headers()["proxy-authorization"], "Basic YWxpY2U6c2VjcmV0");
                        }
                        response.headers_mut().insert("sec-websocket-protocol", "gproxy-test".parse().unwrap());
                        Ok(response)
                    }).await.unwrap();
                    for _ in 0..2 {
                        let message = ws.next().await.unwrap().unwrap();
                        assert!(message.is_text() || message.is_binary());
                        ws.send(message).await.unwrap();
                    }
                    assert!(ws.next().await.unwrap().unwrap().is_close());
                    ws.flush().await.unwrap();
                });
                let pool = ClientPool::default();
                let config = ConnectionConfig {
                    backend,
                    proxy: if proxied { ProxyConfig::Explicit { url: format!("http://alice:secret@{address}") } } else { ProxyConfig::Direct },
                    emulation: (backend == Backend::Wreq).then(|| gproxy_client::EmulationConfig::Preset {
                        profile: "chrome_133".into(), platform: "linux".into(), http2: true, headers: false,
                    }),
                    ..Default::default()
                };
                let client = pool.get_websocket(&config).await.unwrap();
                assert!(Arc::ptr_eq(&client, &pool.get_websocket(&config).await.unwrap()));
                assert!(!Arc::ptr_eq(&client, &pool.get(&config).await.unwrap()));
                let url = if proxied { "ws://origin.invalid/chat".to_owned() } else { format!("ws://{address}/chat") };
                match client.as_ref() {
                    #[cfg(feature = "reqwest-native")]
                    Client::ReqwestNative(_) => unreachable!("not exercised here"),
                    Client::Host(_) => unreachable!("this pool builds its own clients"),
                    #[cfg(feature = "reqwest")]
                    Client::Reqwest(client) => {
                        use gproxy_client::reqwest_websocket::{Upgrade, Message, CloseCode};
                        let response = client.get(&url).bearer_auth("upstream-token").upgrade().protocols(["gproxy-test"]).send().await.unwrap();
                        assert_eq!(response.status(), 101);
                        let mut ws = response.into_websocket().await.unwrap();
                        assert_eq!(ws.protocol(), Some("gproxy-test"));
                        pool.clear();
                        ws.send(Message::Text("hello".into())).await.unwrap();
                        assert!(matches!(ws.next().await.unwrap().unwrap(), Message::Text(text) if text == "hello"));
                        ws.send(Message::Binary(vec![1, 2, 3].into())).await.unwrap();
                        assert!(matches!(ws.next().await.unwrap().unwrap(), Message::Binary(bytes) if bytes.as_ref() == [1, 2, 3]));
                        ws.close(CloseCode::Normal, None).await.unwrap();
                    }
                    #[cfg(feature = "wreq")]
                    Client::Wreq(client) => {
                        use gproxy_client::wreq::ws::message::Message;
                        let response = client.websocket(url.as_str()).bearer_auth("upstream-token").protocols(["gproxy-test"]).send().await.unwrap();
                        assert_eq!(response.status(), 101);
                        let mut ws = response.into_websocket().await.unwrap();
                        assert_eq!(ws.protocol().unwrap(), "gproxy-test");
                        pool.clear();
                        ws.send(Message::Text("hello".into())).await.unwrap();
                        assert!(matches!(ws.recv().await.unwrap().unwrap(), Message::Text(text) if text.as_str() == "hello"));
                        ws.send(Message::Binary(vec![1, 2, 3].into())).await.unwrap();
                        assert!(matches!(ws.recv().await.unwrap().unwrap(), Message::Binary(bytes) if bytes.as_ref() == [1, 2, 3]));
                        ws.close(1000, "").await.unwrap();
                    }
                }
                server.await.unwrap();
            }
        }
    }).await.unwrap();
}
