//! Workers upgrades use the same router and pump as native sockets. The worker
//! crate's HTTP adapter takes the client socket from response extensions.
use super::*;

#[derive(Debug)]
pub struct Upgrade(Arc<worker::Context>);

pub async fn extract(
    parts: &mut http::request::Parts,
    _max_frame_bytes: u64,
) -> Result<Upgrade, Box<Response>> {
    if parts.method != http::Method::GET
        || !parts
            .headers
            .get(header::UPGRADE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        return Err(Box::new(upgrade_required(
            "this surface is a websocket; send an Upgrade request",
        )));
    }
    if parts
        .headers
        .get(header::SEC_WEBSOCKET_VERSION)
        .is_some_and(|value| value != "13")
    {
        return Err(Box::new(
            crate::ErrorResponse(AppError::invalid("unsupported WebSocket version"))
                .into_response(),
        ));
    }
    parts
        .extensions
        .remove::<Arc<worker::Context>>()
        .map(Upgrade)
        .ok_or_else(|| {
            Box::new(
                crate::ErrorResponse(AppError::internal("Workers execution context is missing"))
                    .into_response(),
            )
        })
}

pub(super) fn accept<C: BatchConnectionTrait + Send + Sync + 'static>(
    upgrade: Upgrade,
    upstream: UpstreamSocket,
    max_frame_bytes: u64,
    trailer: Option<Trailer<C>>,
    negotiated: &HeaderMap,
) -> Response {
    let sockets = (|| {
        let pair = worker::WebSocketPair::new().map_err(|error| error.to_string())?;
        let downstream = gproxy_client::accept_workers_websocket(pair.server.as_ref().clone())
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((pair.client, downstream))
    })();
    let (client, downstream) = match sockets {
        Ok(sockets) => sockets,
        Err(error) => return crate::ErrorResponse(AppError::internal(error)).into_response(),
    };
    let mut response = StatusCode::SWITCHING_PROTOCOLS.into_response();
    *response.headers_mut() = negotiated.clone();
    response.extensions_mut().insert(client);
    upgrade
        .0
        .wait_until(Pump::new(downstream, upstream, max_frame_bytes, trailer).run());
    response
}

pub(super) async fn timeout(future: impl std::future::Future<Output = ()>) -> bool {
    use futures_util::future::{Either, select};
    let future = std::pin::pin!(future);
    let timer = std::pin::pin!(gloo_timers::future::TimeoutFuture::new(
        CLOSE_GRACE.as_millis() as u32
    ));
    matches!(select(future, timer).await, Either::Left(_))
}
