use axum::extract::ws::{CloseFrame, Message, WebSocket};
use gproxy_channel_api::{WsDuplex, WsFrame};
use tokio::sync::OwnedSemaphorePermit;

pub(crate) async fn pump(
    mut downstream: WebSocket,
    mut upstream: Box<dyn WsDuplex>,
    permit: OwnedSemaphorePermit,
) {
    let _permit = permit;
    loop {
        tokio::select! {
            message = downstream.recv() => {
                let Some(message) = message else {
                    close_upstream(upstream.as_mut()).await;
                    return;
                };
                let Ok(message) = message else {
                    close_upstream(upstream.as_mut()).await;
                    return;
                };
                match message {
                    Message::Text(text) => {
                        if upstream
                            .send(WsFrame::Text(text.as_str().to_owned()))
                            .await
                            .is_err()
                        {
                            close_downstream(&mut downstream, None).await;
                            return;
                        }
                    }
                    Message::Binary(bytes) => {
                        if upstream.send(WsFrame::Binary(bytes)).await.is_err() {
                            close_downstream(&mut downstream, None).await;
                            return;
                        }
                    }
                    Message::Close(frame) => {
                        let code = frame.map(|frame| frame.code);
                        let _ = upstream.send(WsFrame::Close(code)).await;
                        return;
                    }
                    Message::Ping(payload) => {
                        if downstream.send(Message::Pong(payload)).await.is_err() {
                            close_upstream(upstream.as_mut()).await;
                            return;
                        }
                    }
                    Message::Pong(_) => {}
                }
            }
            frame = upstream.recv() => {
                match frame {
                    Ok(Some(WsFrame::Text(text))) => {
                        if downstream.send(Message::Text(text.into())).await.is_err() {
                            close_upstream(upstream.as_mut()).await;
                            return;
                        }
                    }
                    Ok(Some(WsFrame::Binary(bytes))) => {
                        if downstream.send(Message::Binary(bytes)).await.is_err() {
                            close_upstream(upstream.as_mut()).await;
                            return;
                        }
                    }
                    Ok(Some(WsFrame::Close(code))) => {
                        close_downstream(&mut downstream, code).await;
                        return;
                    }
                    Ok(None) | Err(_) => {
                        close_downstream(&mut downstream, None).await;
                        return;
                    }
                }
            }
        }
    }
}

async fn close_downstream(downstream: &mut WebSocket, code: Option<u16>) {
    let frame = code.map(|code| CloseFrame {
        code,
        reason: "".into(),
    });
    let _ = downstream.send(Message::Close(frame)).await;
}

async fn close_upstream(upstream: &mut dyn WsDuplex) {
    let _ = upstream.send(WsFrame::Close(None)).await;
}
