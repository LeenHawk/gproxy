use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

use futures_core::Stream;
use futures_sink::Sink;
use gproxy_protocol::connection::{
    Bytes, HeaderMap, HeaderValue, HttpBody, Method, Multipart, MultipartPart, StatusCode,
    TransportError, WebSocket, WireRequest, WireResponse, WsFrame,
};

struct Items<T>(VecDeque<T>);

impl<T: Unpin> Stream for Items<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<T>> {
        Poll::Ready(self.0.pop_front())
    }
}

fn next<T>(mut stream: Pin<&mut dyn Stream<Item = T>>) -> Option<T> {
    let mut cx = Context::from_waker(Waker::noop());
    match stream.as_mut().poll_next(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture stream is immediately ready"),
    }
}

#[test]
fn raw_http_keeps_query_encoding_headers_and_arbitrary_body_chunks() {
    let chunks = [
        Bytes::from_static(b"data: {\"text\":\""),
        Bytes::from_static(&[0xe4]),
        Bytes::from_static(&[0xbd, 0xa0]),
        Bytes::from_static(b"\"}\n\n"),
    ];
    let mut headers = HeaderMap::new();
    headers.append("x-extra", HeaderValue::from_static("first"));
    headers.append("x-extra", HeaderValue::from_static("second"));
    headers.insert(
        "content-type",
        HeaderValue::from_static("text/event-stream"),
    );
    let response = WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Stream(Box::pin(Items(chunks.iter().cloned().map(Ok).collect()))),
    };
    let HttpBody::Stream(mut body) = response.body else {
        unreachable!()
    };
    for expected in chunks {
        assert_eq!(next(body.as_mut()).unwrap().unwrap(), expected);
    }
    assert!(next(body.as_mut()).is_none());
    assert_eq!(response.headers.get_all("x-extra").iter().count(), 2);

    let request = WireRequest {
        method: Method::GET,
        path: "/models/a%2Fb".into(),
        query: Some("q=a+b&q=a%20b&flag".into()),
        headers: HeaderMap::new(),
        body: (),
    };
    assert_eq!(request.path, "/models/a%2Fb");
    assert_eq!(request.query.as_deref(), Some("q=a+b&q=a%20b&flag"));
}

#[test]
fn multipart_preserves_repeated_parts_and_streaming_binary_content() {
    let mut parts = VecDeque::new();
    for value in [b"one".as_slice(), &[0, 255, 42]] {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-disposition",
            HeaderValue::from_static("form-data; name=\"file\"; filename=\"a.bin\""),
        );
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/octet-stream"),
        );
        parts.push_back(Ok(MultipartPart {
            headers,
            body: HttpBody::Stream(Box::pin(Items([Ok(Bytes::copy_from_slice(value))].into()))),
        }));
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=test"),
    );
    let mut request = WireRequest {
        method: Method::POST,
        path: "/v1/files".into(),
        query: None,
        headers,
        body: Multipart {
            parts: Box::pin(Items(parts)),
        },
    };
    for expected in [b"one".as_slice(), &[0, 255, 42]] {
        let part = next(request.body.parts.as_mut()).unwrap().unwrap();
        assert_eq!(
            part.headers["content-disposition"],
            "form-data; name=\"file\"; filename=\"a.bin\""
        );
        let HttpBody::Stream(mut content) = part.body else {
            unreachable!()
        };
        assert_eq!(next(content.as_mut()).unwrap().unwrap().as_ref(), expected);
    }
    assert!(next(request.body.parts.as_mut()).is_none());
}

struct Sent(Arc<Mutex<Vec<WsFrame>>>);

impl Sink<WsFrame> for Sent {
    type Error = TransportError;
    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn start_send(self: Pin<&mut Self>, item: WsFrame) -> Result<(), Self::Error> {
        self.0.lock().unwrap().push(item);
        Ok(())
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[test]
fn websocket_can_send_multiple_messages_before_receiving() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut socket = WebSocket {
        incoming: Box::pin(Items(
            [Ok(WsFrame::Ping(Bytes::from_static(b"ping")))].into(),
        )),
        outgoing: Box::pin(Sent(sent.clone())),
    };
    let mut cx = Context::from_waker(Waker::noop());
    for text in ["first", "second"] {
        assert!(matches!(
            socket.outgoing.as_mut().poll_ready(&mut cx),
            Poll::Ready(Ok(()))
        ));
        socket
            .outgoing
            .as_mut()
            .start_send(WsFrame::Text(text.into()))
            .unwrap();
    }
    assert_eq!(sent.lock().unwrap().len(), 2);
    assert_eq!(
        next(socket.incoming.as_mut()).unwrap().unwrap(),
        WsFrame::Ping(Bytes::from_static(b"ping"))
    );
}
