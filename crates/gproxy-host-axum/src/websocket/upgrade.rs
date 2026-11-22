use super::*;
use axum::extract::{
    FromRequestParts as _,
    ws::{CloseFrame, Message, WebSocket as ClientSocket, WebSocketUpgrade},
};
use gproxy_protocol::connection::TransportError;

/// Take the upgrade out of the request, or say why this is not one.
///
/// Runs **after** authentication and before anything is accepted: an
/// anonymous caller learns nothing about this gateway's transports, and a
/// client that did authenticate is told plainly what is wrong with its
/// handshake. Nothing is upgraded here — the `101` is only written by
/// [`WebSocketUpgrade::on_upgrade`] — so a request that is refused after this
/// point is still refused over HTTP.
///
/// `max_frame_bytes` becomes the codec's message cap, one byte over the
/// configured limit, so a frame exactly at the limit is accepted and one over
/// it is caught by this host (with a close code the client can read) rather
/// than by tungstenite (with a protocol error it cannot). The codec is still
/// the memory bound: nothing larger than that is ever buffered.
pub async fn extract(
    parts: &mut http::request::Parts,
    max_frame_bytes: u64,
) -> Result<Upgrade, Box<Response>> {
    let upgrade = WebSocketUpgrade::from_request_parts(parts, &())
        .await
        .map_err(|rejection| Box::new(reject(rejection)))?;
    let max = usize::try_from(max_frame_bytes.saturating_add(1)).unwrap_or(usize::MAX);
    Ok(Upgrade(upgrade.max_message_size(max).max_frame_size(max)))
}

/// The extracted handshake, so a caller cannot confuse it with anything else
/// and cannot accidentally accept it without a decision.
pub struct Upgrade(WebSocketUpgrade);

impl std::fmt::Debug for Upgrade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Upgrade { .. }")
    }
}

/// Write the `101` and start the pump.
///
/// The upstream's negotiated headers are appended to axum's response without
/// overwriting any of its own: a subprotocol the upstream chose has to reach
/// the client, and `sec-websocket-accept` must stay the one computed from the
/// client's key.
pub(super) fn accept<C>(
    upgrade: Upgrade,
    upstream: UpstreamSocket,
    max_frame_bytes: u64,
    trailer: Option<Trailer<C>>,
    negotiated: &HeaderMap,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let mut response = upgrade.0.on_upgrade(move |downstream| {
        Pump::new(socket(downstream), upstream, max_frame_bytes, trailer).run()
    });
    let headers = response.headers_mut();
    for name in negotiated.keys() {
        if headers.contains_key(name) {
            continue;
        }
        for value in negotiated.get_all(name) {
            headers.append(name, value.clone());
        }
    }
    response
}

/// The refusal a malformed handshake earns.
///
/// Axum's own rejections render as `405`, `400` and `426`; this maps them onto
/// the two answers a client can act on, in this crate's error envelope:
///
/// - **`426`** when the request is not an upgrade this connection can serve —
///   the wrong method, no `Connection: upgrade`, no `Upgrade: websocket`, or a
///   connection that cannot be upgraded at all (HTTP/1.0, or HTTP/2 without
///   extended CONNECT). All of those are answered by *re-sending* the request
///   as a handshake over HTTP/1.1, which is exactly what `426` plus
///   `Upgrade: websocket` tells a client to do. A `405` would be wrong:
///   the path is not method-restricted, it is transport-restricted.
/// - **`400`** when it *is* a handshake and is malformed — a missing
///   `Sec-WebSocket-Key`, a version that is not 13. Re-sending it unchanged
///   will not help.
fn reject(rejection: axum::extract::ws::rejection::WebSocketUpgradeRejection) -> Response {
    use axum::extract::ws::rejection::WebSocketUpgradeRejection as R;
    match rejection {
        R::MethodNotGet(_)
        | R::MethodNotConnect(_)
        | R::InvalidConnectionHeader(_)
        | R::InvalidUpgradeHeader(_)
        | R::InvalidProtocolPseudoheader(_)
        | R::ConnectionNotUpgradable(_) => {
            upgrade_required("this surface is a websocket; send an Upgrade request")
        }
        other => crate::ErrorResponse(AppError::invalid(format!(
            "websocket handshake: {}",
            other.body_text()
        )))
        .into_response(),
    }
}

fn socket(socket: ClientSocket) -> UpstreamSocket {
    let (outgoing, incoming) = socket.split();
    UpstreamSocket {
        incoming: Box::pin(incoming.map(|message| {
            message
                .map(|message| match message {
                    Message::Text(text) => WsFrame::Text(text.to_string()),
                    Message::Binary(bytes) => WsFrame::Binary(bytes),
                    Message::Ping(bytes) => WsFrame::Ping(bytes),
                    Message::Pong(bytes) => WsFrame::Pong(bytes),
                    Message::Close(close) => WsFrame::Close(close.map(|close| WsClose {
                        code: close.code,
                        reason: close.reason.to_string(),
                    })),
                })
                .map_err(|error| Box::new(error) as TransportError)
        })),
        outgoing: Box::pin(
            outgoing
                .with(|frame| async move {
                    Ok::<_, axum::Error>(match frame {
                        WsFrame::Text(text) => Message::Text(text.into()),
                        WsFrame::Binary(bytes) => Message::Binary(bytes),
                        WsFrame::Ping(bytes) => Message::Ping(bytes),
                        WsFrame::Pong(bytes) => Message::Pong(bytes),
                        WsFrame::Close(close) => Message::Close(close.map(|close| CloseFrame {
                            code: close.code,
                            reason: close.reason.into(),
                        })),
                    })
                })
                .sink_map_err(|error| Box::new(error) as TransportError),
        ),
    }
}

pub(super) async fn timeout(future: impl std::future::Future<Output = ()>) -> bool {
    tokio::time::timeout(CLOSE_GRACE, future).await.is_ok()
}
