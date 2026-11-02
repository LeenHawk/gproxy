use crate::Result;
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture,
};

/// Two implementations share one caller API. This is also the injection point
/// for applications that already own a transport.
pub trait Backend {
    /// Network mount used to normalize gateway-owned upload session URLs.
    /// In-process backends return None and use origin-relative session paths.
    fn endpoint(&self) -> Option<&str> {
        None
    }
    #[cfg(feature = "realtime")]
    fn connect(
        &self,
        _operation: OperationKey,
        _model: &str,
        _request: WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
        Box::pin(async {
            Err(crate::Error::invalid(
                "this backend does not support realtime connections",
            ))
        })
    }
    fn send(
        &self,
        operation: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>>;
}

impl<T: Backend + ?Sized> Backend for Box<T> {
    fn endpoint(&self) -> Option<&str> {
        (**self).endpoint()
    }
    #[cfg(feature = "realtime")]
    fn connect(
        &self,
        operation: OperationKey,
        model: &str,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
        (**self).connect(operation, model, request)
    }
    fn send(
        &self,
        operation: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        (**self).send(operation, model, request)
    }
}

#[cfg(feature = "remote")]
mod remote {
    use super::*;
    use crate::Error;
    use gproxy_client::{ClientPool, ConnectionConfig, OutboundClient};
    use http::HeaderValue;
    use std::sync::Arc;

    pub struct Remote {
        endpoint: String,
        key: HeaderValue,
        transport: Arc<dyn OutboundClient>,
    }
    impl Remote {
        /// `endpoint` is the deployment origin plus any mount prefix, e.g.
        /// `https://gproxy.example/channel`. Paths below it include `/v1`.
        pub async fn new(endpoint: impl Into<String>, gateway_key: &str) -> Result<Self> {
            let pool = ClientPool::default();
            let config = ConnectionConfig::default();
            let transport = pool.get(&config).await.map_err(Error::network)?;
            Self::with_transport(endpoint, gateway_key, transport)
        }
        pub fn with_transport(
            endpoint: impl Into<String>,
            gateway_key: &str,
            transport: Arc<dyn OutboundClient>,
        ) -> Result<Self> {
            let mut key =
                HeaderValue::from_str(&format!("Bearer {gateway_key}")).map_err(Error::invalid)?;
            key.set_sensitive(true);
            Ok(Self {
                endpoint: endpoint.into().trim_end_matches('/').into(),
                key,
                transport,
            })
        }
    }
    impl Backend for Remote {
        fn endpoint(&self) -> Option<&str> {
            Some(&self.endpoint)
        }
        #[cfg(feature = "realtime")]
        fn connect(
            &self,
            _operation: OperationKey,
            model: &str,
            request: WireRequest<()>,
        ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
            let hint = HeaderValue::from_str(&crate::path_segment(model)).map_err(Error::invalid);
            Box::pin(async move {
                let mut url = format!("{}{}", self.endpoint, request.path);
                if let Some(query) = request.query {
                    url.push('?');
                    url.push_str(&query);
                }
                let mut outbound = http::Request::builder()
                    .method(request.method)
                    .uri(url)
                    .body(())
                    .map_err(Error::invalid)?;
                *outbound.headers_mut() = request.headers;
                outbound.headers_mut().insert("x-gproxy-model", hint?);
                outbound
                    .headers_mut()
                    .insert(http::header::AUTHORIZATION, self.key.clone());
                self.transport
                    .connect(outbound)
                    .await
                    .map_err(Error::network)
            })
        }
        fn send(
            &self,
            operation: OperationKey,
            model: Option<&str>,
            request: WireRequest<HttpBody>,
        ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
            let model_header = if matches!(
                operation.operation,
                gproxy_protocol::Operation::CreateFile
                    | gproxy_protocol::Operation::CreateRealtimeCall
            ) {
                model
                    .map(|model| {
                        HeaderValue::from_str(&crate::path_segment(model)).map_err(Error::invalid)
                    })
                    .transpose()
            } else {
                Ok(None)
            };
            Box::pin(async move {
                let mut url = format!("{}{}", self.endpoint, request.path);
                if let Some(query) = request.query {
                    url.push('?');
                    url.push_str(&query);
                }
                let mut outbound = http::Request::builder()
                    .method(request.method)
                    .uri(url)
                    .body(request.body)
                    .map_err(Error::invalid)?;
                *outbound.headers_mut() = request.headers;
                if let Some(model) = model_header? {
                    outbound.headers_mut().insert("x-gproxy-model", model);
                }
                outbound
                    .headers_mut()
                    .insert(http::header::AUTHORIZATION, self.key.clone());
                self.transport.send(outbound).await.map_err(Error::network)
            })
        }
    }
}
#[cfg(feature = "remote")]
pub use remote::Remote;

#[cfg(all(feature = "embedded", not(target_arch = "wasm32")))]
mod embedded {
    use super::*;
    use crate::Error;
    use futures_util::{StreamExt, stream};
    use gproxy_sdk::Gproxy;
    use gproxy_seaorm::BatchConnectionTrait;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    pub struct Embedded<C> {
        engine: Arc<Gproxy<C>>,
        scope: String,
    }
    impl<C> Embedded<C> {
        pub fn new(engine: Arc<Gproxy<C>>, scope: impl Into<String>) -> Self {
            Self {
                engine,
                scope: scope.into(),
            }
        }
    }
    impl<C: BatchConnectionTrait + Send + Sync + 'static> Backend for Embedded<C> {
        #[cfg(feature = "realtime")]
        fn connect(
            &self,
            operation: OperationKey,
            model: &str,
            request: WireRequest<()>,
        ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
            let model = model.to_owned();
            Box::pin(async move {
                let cancellation = CancellationToken::new();
                let guard = cancellation.clone().drop_guard();
                let execution = self
                    .engine
                    .connect(operation, request)
                    .scope(&self.scope)
                    .model(model)
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
                match connection {
                    gproxy_protocol::capability::UpstreamConnection::Connected {
                        handshake,
                        mut socket,
                    } => {
                        socket.incoming = Box::pin(stream::unfold(
                            (socket.incoming, guard, completion),
                            |(mut incoming, guard, completion)| async move {
                                incoming
                                    .next()
                                    .await
                                    .map(|frame| (frame, (incoming, guard, completion)))
                            },
                        ));
                        Ok(gproxy_protocol::capability::UpstreamConnection::Connected {
                            handshake,
                            socket,
                        })
                    }
                    rejected => Ok(rejected),
                }
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
                if operation.operation == gproxy_protocol::Operation::ListModels {
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
                    let data: Vec<_> = names
                        .into_iter()
                        .map(|id| {
                            serde_json::json!({
                                "id": id,
                                "object": "model",
                                "created": 0,
                                "owned_by": "gproxy"
                            })
                        })
                        .collect();
                    return Ok(WireResponse {
                        status: http::StatusCode::OK,
                        headers: http::HeaderMap::new(),
                        body: HttpBody::Bytes(
                            serde_json::to_vec(&serde_json::json!({"object":"list","data":data}))
                                .map_err(Error::invalid)?
                                .into(),
                        ),
                    });
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
}
#[cfg(all(feature = "embedded", not(target_arch = "wasm32")))]
pub use embedded::Embedded;
