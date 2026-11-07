//! A `gproxy-client-sdk` backend that drives this handle in process.
//!
//! It lives here rather than in the client SDK so that the MIT-licensed client
//! does not depend on the engine: this crate depends on the client instead.

use crate::Gproxy;
use futures_util::{StreamExt, stream};
use gproxy_client_sdk::{Error, Result, backend::Backend};
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture,
};
use gproxy_seaorm::BatchConnectionTrait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Serves a `gproxy_client_sdk::Client` from an in-process [`Gproxy`]
/// instead of over HTTP. Calls run under `scope`; access policy is the
/// embedding application's responsibility.
pub struct ClientBackend<C> {
    engine: Arc<Gproxy<C>>,
    scope: String,
}
impl<C> ClientBackend<C> {
    pub fn new(engine: Arc<Gproxy<C>>, scope: impl Into<String>) -> Self {
        Self {
            engine,
            scope: scope.into(),
        }
    }
}
impl<C: BatchConnectionTrait + Send + Sync + 'static> Backend for ClientBackend<C> {
    fn connect(
        &self,
        operation: OperationKey,
        model: &str,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
        let model = model.to_owned();
        Box::pin(async move {
            use gproxy_protocol::{capability::UpstreamConnection, connection::WebSocket};
            let cancellation = CancellationToken::new();
            let guard = cancellation.clone().drop_guard();
            let execution = self
                .engine
                .connect(operation, request)
                .model(model)
                .scope(&self.scope)
                .cancellation(cancellation)
                .send()
                .await
                .map_err(|error| {
                    Error::from_response(
                        error.status_code(),
                        &http::HeaderMap::new(),
                        error.to_string(),
                    )
                })?;
            let (connection, completion) = execution.into_parts();
            Ok(match connection {
                UpstreamConnection::Connected { handshake, socket } => {
                    let incoming = stream::unfold(
                        (socket.incoming, guard, completion),
                        |(mut incoming, guard, completion)| async move {
                            incoming
                                .next()
                                .await
                                .map(|frame| (frame, (incoming, guard, completion)))
                        },
                    );
                    UpstreamConnection::Connected {
                        handshake,
                        socket: WebSocket {
                            incoming: Box::pin(incoming),
                            outgoing: socket.outgoing,
                        },
                    }
                }
                rejected => rejected,
            })
        })
    }
    fn send(
        &self,
        operation: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.map(str::to_owned);
        Box::pin(async move {
            // The HTTP host serves this from the local catalog, before
            // calling upstream. Embedded callers see the same name forms;
            // access policy is the embedding host's responsibility.
            if matches!(
                operation.operation,
                gproxy_protocol::Operation::ListModels | gproxy_protocol::Operation::GetModel
            ) {
                let routing = self.engine.routing();
                let core = self.engine.core().snapshot();
                let mut names: std::collections::BTreeSet<String> = routing
                    .names
                    .iter()
                    .filter(|(_, route_id)| {
                        routing.routes[*route_id]
                            .members
                            .iter()
                            .any(|m| core.providers.contains_key(&m.provider_id))
                    })
                    .map(|(name, _)| name.clone())
                    .collect();
                for provider in core.providers.values() {
                    for model in &provider.models {
                        names.extend(
                            model
                                .exposed_names()
                                .into_iter()
                                .map(|name| format!("{}/{name}", provider.entity.name)),
                        );
                    }
                }
                return catalog(operation, model.as_deref(), &names);
            }
            let cancellation = CancellationToken::new();
            let guard = cancellation.clone().drop_guard();
            let mut call = self
                .engine
                .call(operation, request)
                .scope(&self.scope)
                .cancellation(cancellation);
            if let Some(model) = model {
                call = call.model(model);
            }
            let execution = call.send().await.map_err(|error| {
                Error::from_response(
                    error.status_code(),
                    &http::HeaderMap::new(),
                    error.to_string(),
                )
            })?;
            let (mut response, completion) = execution.into_parts();
            if let HttpBody::Stream(bytes) = response.body {
                response.body = HttpBody::Stream(Box::pin(stream::unfold(
                    (bytes, guard, completion),
                    |(mut bytes, guard, completion)| async move {
                        bytes
                            .next()
                            .await
                            .map(|chunk| (chunk, (bytes, guard, completion)))
                    },
                )));
            }
            Ok(response)
        })
    }
}

/// The catalog shapes the HTTP host serves, per dialect, names only.
fn catalog(
    operation: OperationKey,
    model: Option<&str>,
    names: &std::collections::BTreeSet<String>,
) -> Result<WireResponse<HttpBody>> {
    use gproxy_protocol::Dialect;
    use serde_json::{Value, json};
    let entry = |id: &str| match operation.dialect {
        Dialect::Claude => json!({
            "id": id, "type": "model", "display_name": id,
            "created_at": "1970-01-01T00:00:00Z",
        }),
        Dialect::Gemini => json!({
            "name": format!("models/{id}"), "baseModelId": id, "displayName": id,
        }),
        _ => json!({"id": id, "object": "model", "created": 0, "owned_by": "gproxy"}),
    };
    let body = if operation.operation == gproxy_protocol::Operation::GetModel {
        let Some(id) = model.filter(|model| names.contains(*model)) else {
            return Err(Error::from_response(
                404,
                &http::HeaderMap::new(),
                "model is not in the local catalog".into(),
            ));
        };
        entry(id)
    } else {
        let models: Vec<Value> = names.iter().map(|id| entry(id)).collect();
        match operation.dialect {
            Dialect::Claude => json!({
                "data": models,
                "first_id": names.first(),
                "last_id": names.last(),
                "has_more": false,
            }),
            Dialect::Gemini => json!({"models": models}),
            _ => json!({"object": "list", "data": models}),
        }
    };
    Ok(WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: HttpBody::Bytes(
            serde_json::to_vec(&body)
                .expect("a JSON value serializes")
                .into(),
        ),
    })
}
