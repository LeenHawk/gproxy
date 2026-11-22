//! WebSocket operations, relayed as they are: Realtime, Live, and Responses
//! over a WebSocket on a provider whose upstream speaks it. Converting a
//! Responses WebSocket client onto an HTTP upstream is the full gateway's.

use futures_util::{SinkExt, StreamExt};
use gproxy_convert::{Route, surface};
use gproxy_protocol::{
    HttpBody, WebSocket, WireRequest, WireResponse,
    capability::UpstreamConnection,
    connection::{WsFrame, WsReceiver, WsSender},
};
use http::StatusCode;

use crate::{Engine, auth, engine::Refusal, exec::Bound};

/// The `model` of a handshake's query: where Realtime names it.
fn query_model(query: Option<&str>) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == "model").then(|| value.to_owned())
    })
}

fn with_model(query: Option<&str>, model: &str) -> Option<String> {
    Some(
        query?
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some(("model", _)) => format!("model={model}"),
                _ => pair.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("&"),
    )
}

impl Engine {
    /// Open the upstream side of a WebSocket handshake. The host accepts the
    /// client's socket once this succeeds and hands both to [`relay`]; a
    /// refusal is the response to answer the handshake with.
    pub async fn connect(
        &self,
        request: http::Request<()>,
    ) -> Result<WebSocket, Box<WireResponse<HttpBody>>> {
        let mut dialect = None;
        let inner = &self.0;
        let outcome: Result<WebSocket, Refusal> = async {
            let (parts, ()) = request.into_parts();
            let query = parts.uri.query();
            if !auth::presented(&parts.headers, query)
                .is_some_and(|key| auth::matches(&key, &inner.access_key))
            {
                return Err(Refusal::new(
                    StatusCode::UNAUTHORIZED,
                    "the access key is missing or wrong",
                ));
            }
            let (mount, remainder) = inner.mount(&parts.method, parts.uri.path());
            let codex =
                mount.is_some_and(|provider| inner.providers[provider].channel.id() == "codex");
            let matched = if codex {
                surface::match_codex_path(&parts.method, remainder, &parts.headers, None)
            } else {
                surface::match_path(&parts.method, remainder, &parts.headers, None)
            }
            .filter(|matched| matched.upgrade)
                .ok_or_else(|| Refusal::new(StatusCode::NOT_FOUND, "no such WebSocket path"))?;
            dialect = Some(matched.operation.dialect);
            let named = query_model(query);
            let addressed = inner.address(mount, named.clone())?;
            let provider = &inner.providers[addressed.provider];
            let route = gproxy_convert::default_route_for_model(
                provider.channel.as_ref(),
                inner.provider_view(addressed.provider),
                matched.operation,
                addressed.model.as_deref(),
            );
            if route != Route::Passthrough {
                return Err(Refusal::new(
                    StatusCode::NOT_IMPLEMENTED,
                    format!(
                        "provider `{}` has no such WebSocket upstream; the lite engine relays WebSockets and does not convert them",
                        provider.name
                    ),
                ));
            }
            let query = match (&addressed.model, &named) {
                (Some(model), Some(named)) if model != named => with_model(query, model),
                _ => query.map(str::to_owned),
            };
            let bound = Bound::new(inner, addressed.provider).await?;
            let connection = bound
                .connect(
                    matched.operation,
                    WireRequest {
                        method: parts.method,
                        path: if codex {
                            surface::codex_path(remainder).into_owned()
                        } else {
                            remainder.to_owned()
                        },
                        query,
                        headers: parts.headers,
                        body: (),
                    },
                )
                .await?;
            match connection {
                UpstreamConnection::Connected { socket, .. } => Ok(socket),
                UpstreamConnection::Rejected(response) => Err(Refusal::new(
                    response.status,
                    "the upstream refused the WebSocket handshake",
                )),
            }
        }
        .await;
        outcome.map_err(|refusal| Box::new(refusal.response(dialect)))
    }
}

async fn forward(mut from: WsReceiver, mut to: WsSender) {
    while let Some(Ok(frame)) = from.next().await {
        let closing = matches!(frame, WsFrame::Close(_));
        if to.send(frame).await.is_err() || closing {
            break;
        }
    }
    let _ = to.close().await;
}

/// Carry frames both ways until either side closes.
pub async fn relay(client: WebSocket, upstream: WebSocket) {
    futures_util::future::join(
        forward(client.incoming, upstream.outgoing),
        forward(upstream.incoming, client.outgoing),
    )
    .await;
}
