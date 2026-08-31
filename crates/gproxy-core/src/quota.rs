//! On-demand quota probe: query a channel's dedicated usage endpoint for one
//! credential and sink the windows through the same observation path as
//! header-borne readings. Callers trigger this explicitly — some upstreams
//! rate-limit their usage endpoints aggressively.

use bytes::BytesMut;
use futures_util::StreamExt;
use gproxy_channel_api::QuotaObservation;

use crate::host::{CredentialId, CredentialStore};
use crate::{Core, CoreError, Host, ProviderRef, UpstreamTransport};

impl<H: Host> Core<H> {
    pub async fn quota_probe(
        &self,
        channel: &str,
        provider: &ProviderRef,
        credential: CredentialId,
    ) -> Result<Vec<QuotaObservation>, CoreError> {
        if provider.channel != channel {
            return Err(CoreError::UnknownProvider(
                "probe provider does not match channel".into(),
            ));
        }
        let channel = self
            .channels
            .get(channel)
            .ok_or_else(|| CoreError::UnknownProvider("channel is not registered".into()))?;
        let record = self.host.credentials().load(credential).await?;
        let Some(mut request) = channel.prepare_quota_probe(&record.secret, &provider.settings)?
        else {
            return Err(CoreError::Unsupported);
        };
        crate::fingerprint::apply_request(&mut request, provider)?;
        let response = self.host.transport().send(request).await?;
        let (parts, mut stream) = response.into_parts();
        let mut body = BytesMut::new();
        while let Some(chunk) = stream.next().await {
            body.extend_from_slice(&chunk?);
        }
        let observations = channel.parse_quota_probe(parts.status, &body);
        if !observations.is_empty() {
            self.host
                .observe_credential_quota(credential, observations.clone())
                .await;
        }
        Ok(observations)
    }
}
