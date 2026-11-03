use super::*;
use crate::Error;
use gproxy_client::{ClientPool, ConnectionConfig, OutboundClient};
use http::HeaderValue;
use std::sync::Arc;

pub struct Remote {
    endpoint: String,
    key: HeaderValue,
    transport: Arc<dyn OutboundClient>,
    #[cfg(feature = "realtime")]
    websocket: Arc<dyn OutboundClient>,
}
impl Remote {
    /// `endpoint` is the deployment origin plus any mount prefix, e.g.
    /// `https://gproxy.example/channel`. Paths below it include `/v1`.
    pub async fn new(endpoint: impl Into<String>, gateway_key: &str) -> Result<Self> {
        let pool = ClientPool::default();
        let config = ConnectionConfig::default();
        let transport = pool
            .get(&config)
            .await
            .map_err(|error| Error::network_cause(&error))?;
        #[cfg(feature = "realtime")]
        {
            let websocket = pool
                .get_websocket(&config)
                .await
                .map_err(|error| Error::network_cause(&error))?;
            let mut remote = Self::with_transport(endpoint, gateway_key, transport)?;
            remote.websocket = websocket;
            Ok(remote)
        }
        #[cfg(not(feature = "realtime"))]
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
            #[cfg(feature = "realtime")]
            websocket: transport.clone(),
            transport,
        })
    }
}
impl Backend for Remote {
    #[cfg(feature = "realtime")]
    fn connect(
        &self,
        _operation: OperationKey,
        _model: &str,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
        Box::pin(async move {
            let origin = if let Some(rest) = self.endpoint.strip_prefix("https://") {
                format!("wss://{rest}")
            } else if let Some(rest) = self.endpoint.strip_prefix("http://") {
                format!("ws://{rest}")
            } else {
                return Err(Error::invalid("remote endpoint must use http or https"));
            };
            let mut url = format!("{origin}{}", request.path);
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
            outbound
                .headers_mut()
                .insert(http::header::AUTHORIZATION, self.key.clone());
            self.websocket.connect(outbound).await.map_err(|error| {
                if error.kind() == gproxy_protocol::capability::CapabilityErrorKind::Unsupported {
                    Error::unsupported(error.to_string())
                } else {
                    Error::network_cause(&error)
                }
            })
        })
    }
    fn send(
        &self,
        _operation: OperationKey,
        _model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
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

            outbound
                .headers_mut()
                .insert(http::header::AUTHORIZATION, self.key.clone());
            self.transport
                .send(outbound)
                .await
                .map_err(|error| Error::network_cause(&error))
        })
    }
}
