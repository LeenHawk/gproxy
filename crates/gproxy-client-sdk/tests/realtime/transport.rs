use super::support::*;
use protocol::{
    capability::{CapabilityFuture, UpstreamConnection},
    connection::WsFrame,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

#[test]
fn rejected_handshake_keeps_status_body_and_default_backend_is_unsupported() {
    futures_executor::block_on(async {
        let mut script = Script::new(Vec::new());
        script.reject = Some((429, "{\"error\":{\"message\":\"wait\"}}".into()));
        let error = Client::new(script)
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .err()
            .unwrap();
        assert_eq!(error.status, Some(429));
        assert_eq!(error.kind, ErrorKind::RateLimit);
        assert_eq!(
            error.body.as_deref(),
            Some("{\"error\":{\"message\":\"wait\"}}")
        );
        struct HttpOnly;
        impl Backend for HttpOnly {
            fn send(
                &self,
                _: protocol::OperationKey,
                _: Option<&str>,
                _: protocol::WireRequest<protocol::HttpBody>,
            ) -> CapabilityFuture<'_, Result<protocol::WireResponse<protocol::HttpBody>>>
            {
                Box::pin(async { panic!("unused") })
            }
        }
        assert_eq!(
            Client::new(HttpOnly)
                .connect_realtime(&config(RealtimeDialect::Openai))
                .await
                .err()
                .unwrap()
                .kind,
            ErrorKind::Unsupported
        );
        let boxed: Box<dyn Backend> = Box::new(Script::new(vec![ready()]));
        let mut events = Client::new(boxed)
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap()
            .events;
        assert!(matches!(
            events.next().await.unwrap().kind,
            RealtimeEventKind::Ready
        ));
    });
}
#[test]
fn malformed_frames_and_unexpected_eof_are_fatal_without_panicking() {
    futures_executor::block_on(async {
        for malformed in [true, false] {
            let script = Script::new(vec![ready()]);
            {
                let mut frames = script.frames.lock().unwrap();
                frames.as_mut().unwrap().pop();
                if malformed {
                    frames
                        .as_mut()
                        .unwrap()
                        .push(WsFrame::Text("not-json".into()));
                }
            }
            let mut events = Client::new(script)
                .connect_realtime(&config(RealtimeDialect::Openai))
                .await
                .unwrap()
                .events;
            assert!(matches!(
                events.next().await.unwrap().kind,
                RealtimeEventKind::Ready
            ));
            assert!(matches!(
                events.next().await.unwrap().kind,
                RealtimeEventKind::Error { fatal: true, .. }
            ));
            assert!(matches!(
                events.next().await.unwrap().kind,
                RealtimeEventKind::Closed { .. }
            ));
            assert!(events.next().await.is_none());
        }
    });
}
struct WakeCount(AtomicUsize);
impl futures_util::task::ArcWake for WakeCount {
    fn wake_by_ref(this: &Arc<Self>) {
        this.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn cloned_sender_wakes_idle_stream_and_close_is_forwarded() {
    futures_executor::block_on(async {
        let mut script = Script::new(vec![ready()]);
        script.pending = true;
        let sent = script.sent.clone();
        let mut session = Client::new(script)
            .connect_realtime(&config(RealtimeDialect::Openai))
            .await
            .unwrap();
        session.events.next().await.unwrap();
        let counter = Arc::new(WakeCount(AtomicUsize::new(0)));
        let waker = futures_util::task::waker(counter.clone());
        let mut context = Context::from_waker(&waker);
        assert!(matches!(
            session.events.poll_next_unpin(&mut context),
            Poll::Pending
        ));
        let sender = session.sender.clone();
        sender.send_text("wake").await.unwrap();
        assert!(counter.0.load(Ordering::SeqCst) > 0);
        assert!(
            matches!(session.events.next().await.unwrap().kind, RealtimeEventKind::InputText { text, .. } if text == "wake")
        );
        sender.close(1000, "done").await.unwrap();
        assert!(
            matches!(sent.lock().unwrap().last(), Some(WsFrame::Close(Some(c))) if c.code == 1000 && c.reason == "done")
        );
        assert!(sender.send_text("closed").await.is_err());
    });
}
#[cfg(feature = "remote")]
#[test]
fn remote_uses_authorized_websocket_upgrade_with_mount_and_encoded_model() {
    use gproxy_client::OutboundClient;
    struct Upgrade {
        script: Script,
        uri: Mutex<String>,
        auth: Mutex<String>,
    }
    impl OutboundClient for Upgrade {
        fn send<'a>(
            &'a self,
            _: http::Request<protocol::HttpBody>,
        ) -> CapabilityFuture<
            'a,
            std::result::Result<
                protocol::WireResponse<protocol::HttpBody>,
                protocol::capability::CapabilityError,
            >,
        > {
            Box::pin(async { panic!("HTTP send must not be used") })
        }
        fn connect<'a>(
            &'a self,
            request: http::Request<()>,
        ) -> CapabilityFuture<
            'a,
            std::result::Result<UpstreamConnection, protocol::capability::CapabilityError>,
        > {
            *self.uri.lock().unwrap() = request.uri().to_string();
            *self.auth.lock().unwrap() = request.headers()["authorization"]
                .to_str()
                .unwrap()
                .to_owned();
            Box::pin(async move {
                Ok(UpstreamConnection::Connected {
                    handshake: protocol::WireResponse {
                        status: http::StatusCode::SWITCHING_PROTOCOLS,
                        headers: http::HeaderMap::new(),
                        body: (),
                    },
                    socket: self.script.socket(),
                })
            })
        }
    }
    futures_executor::block_on(async {
        let upstream = Arc::new(Upgrade {
            script: Script::new(vec![ready()]),
            uri: Mutex::default(),
            auth: Mutex::default(),
        });
        let remote = backend::Remote::with_transport(
            "https://gateway.example/mount/",
            "gateway-key",
            upstream.clone(),
        )
        .unwrap();
        let mut c = config(RealtimeDialect::Openai);
        c.model.id = "route/a?b&c".into();
        let mut session = Client::new(remote).connect_realtime(&c).await.unwrap();
        assert!(matches!(
            session.events.next().await.unwrap().kind,
            RealtimeEventKind::Ready
        ));
        assert_eq!(
            *upstream.uri.lock().unwrap(),
            "wss://gateway.example/mount/v1/realtime?model=route%2Fa%3Fb%26c"
        );
        assert_eq!(*upstream.auth.lock().unwrap(), "Bearer gateway-key");
        assert_eq!(upstream.script.values()[0]["type"], "session.update");
    });
}
