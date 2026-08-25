use futures_util::future::{Either, select};
use futures_util::pin_mut;
use gproxy_channel_api::{TransportError, WsDuplex, WsFrame};

use super::DownstreamSocket;

pub(super) async fn run(mut downstream: DownstreamSocket, mut upstream: Box<dyn WsDuplex>) {
    enum Event {
        Downstream(Result<Option<WsFrame>, wasm_bindgen::JsValue>),
        Upstream(Result<Option<WsFrame>, TransportError>),
    }

    loop {
        let event = {
            let downstream_frame = downstream.recv();
            let upstream_frame = upstream.recv();
            pin_mut!(downstream_frame, upstream_frame);
            match select(downstream_frame, upstream_frame).await {
                Either::Left((frame, _)) => Event::Downstream(frame),
                Either::Right((frame, _)) => Event::Upstream(frame),
            }
        };
        match event {
            Event::Downstream(frame) => match frame {
                Ok(Some(WsFrame::Text(text))) => {
                    if upstream.send(WsFrame::Text(text)).await.is_err() {
                        let _ = downstream.send(WsFrame::Close(None));
                        return;
                    }
                }
                Ok(Some(WsFrame::Binary(bytes))) => {
                    if upstream.send(WsFrame::Binary(bytes)).await.is_err() {
                        let _ = downstream.send(WsFrame::Close(None));
                        return;
                    }
                }
                Ok(Some(WsFrame::Close(code))) => {
                    let _ = upstream.send(WsFrame::Close(code)).await;
                    return;
                }
                Ok(None) | Err(_) => {
                    let _ = upstream.send(WsFrame::Close(None)).await;
                    return;
                }
            },
            Event::Upstream(frame) => match frame {
                Ok(Some(frame)) => {
                    let closed = matches!(frame, WsFrame::Close(_));
                    if downstream.send(frame).is_err() || closed {
                        if !closed {
                            let _ = upstream.send(WsFrame::Close(None)).await;
                        }
                        return;
                    }
                }
                Ok(None) | Err(_) => {
                    let _ = downstream.send(WsFrame::Close(None));
                    return;
                }
            },
        }
    }
}
