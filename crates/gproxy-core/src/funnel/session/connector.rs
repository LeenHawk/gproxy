use std::sync::Arc;

use bytes::Bytes;
use gproxy_channel_api::{
    Channel, PreparedRequest, RealtimeMeter, SessionPrepareCtx, SessionPreparer, WsDuplex,
};

use crate::control::Target;
use crate::error::CoreError;
use crate::host::{Host, UpstreamTransport};

#[derive(Clone)]
pub(super) struct Connector {
    channel: Arc<dyn Channel>,
    target: Target,
    prepare: SessionPreparer,
    request_body: Bytes,
    request_headers: http::HeaderMap,
    response_headers: http::HeaderMap,
}

pub(super) struct Prepared {
    pub id: String,
    pub request: PreparedRequest,
    pub meter: RealtimeMeter,
    pub credential_version: u64,
}

pub(super) struct Attempt {
    pub url: String,
    pub body: Bytes,
    pub opened: Result<Box<dyn WsDuplex>, gproxy_channel_api::TransportError>,
    pub meter: RealtimeMeter,
    pub credential_version: u64,
}

impl Connector {
    pub(super) fn new(
        channel: Arc<dyn Channel>,
        target: Target,
        request_body: Bytes,
        request_headers: http::HeaderMap,
        response_headers: http::HeaderMap,
    ) -> Self {
        let prepare = channel
            .session_preparer()
            .expect("session meter capability was checked at startup");
        Self {
            channel,
            target,
            prepare,
            request_body,
            request_headers,
            response_headers,
        }
    }

    pub(super) async fn prepare<H: Host>(
        &self,
        host: &H,
        force_refresh: bool,
    ) -> Result<Prepared, CoreError> {
        let credential = if force_refresh {
            crate::execution::credential::refresh_now(
                host,
                self.channel.as_ref(),
                self.target.credential,
                &self.target.provider,
            )
            .await?
        } else {
            crate::execution::credential::load_fresh(
                host,
                self.channel.as_ref(),
                self.target.credential,
                &self.target.provider,
            )
            .await?
        };
        let mut prepared = (self.prepare)(SessionPrepareCtx {
            request_body: &self.request_body,
            request_headers: &self.request_headers,
            response_headers: &self.response_headers,
            upstream_model: &self.target.upstream_model,
            secret: &credential.secret,
        })?;
        if !prepared.request.websocket {
            return Err(CoreError::Internal(
                "session observer was not prepared as a websocket".into(),
            ));
        }
        crate::fingerprint::apply_prepared(&mut prepared.request, &self.target.provider)?;
        Ok(Prepared {
            id: prepared.id,
            request: prepared.request,
            meter: prepared.meter,
            credential_version: credential.version,
        })
    }
}

impl Prepared {
    pub(super) async fn open<H: Host>(self, host: &H) -> Attempt {
        let url = self.request.request.uri().to_string();
        let body = self.request.request.body().clone();
        let opened = host.transport().open_websocket(self.request.request).await;
        Attempt {
            url,
            body,
            opened,
            meter: self.meter,
            credential_version: self.credential_version,
        }
    }
}
