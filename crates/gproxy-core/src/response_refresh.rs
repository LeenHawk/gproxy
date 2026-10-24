//! Persist server-initiated credential rotations below channel execution and
//! capture, using the same sealed Store CAS and live version publication as
//! explicit refresh. Channel-specific headers and token formats stay in the
//! existing CredentialRefresh ability.
use crate::{
    Core, CoreData, CredentialState, CredentialStatus, CredentialVersion, Invalidation,
    SecretCodec, keys,
};
use gproxy_channel::{BaseChannel, OutboundClient, channel::CredentialView};
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        UpstreamConnection,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Store,
    operations::{CasOutcome, credentials::CredentialRefresh},
};
use std::sync::Arc;

struct ResponseRefresh<C> {
    inner: Arc<dyn OutboundClient>,
    channel: Arc<dyn BaseChannel>,
    id: String,
    provider_id: String,
    auth_kind: String,
    state: Arc<CredentialState>,
    store: Arc<Store<C>>,
    codec: Arc<dyn SecretCodec>,
    cache: Arc<dyn gproxy_cache::Cache>,
}
fn storage(error: impl std::fmt::Display) -> CapabilityError {
    CapabilityError::new(
        CapabilityErrorKind::Storage,
        CapabilityErrorStage::Start,
        format!("credential rotation persistence failed: {error}"),
    )
}
impl<C: BatchConnectionTrait + Send + Sync> OutboundClient for ResponseRefresh<C> {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let current = self.state.load();
            let mut head = http::Request::builder()
                .method(request.method())
                .uri(request.uri())
                .body(())
                .map_err(storage)?;
            *head.headers_mut() = request.headers().clone();
            let mut response = self.inner.send(request).await?;
            let Some(refresh) = self.channel.credential_refresh() else {
                return Ok(response);
            };
            let update = refresh.response_update(
                CredentialView {
                    id: &self.id,
                    provider_id: &self.provider_id,
                    auth_kind: &self.auth_kind,
                    secret: &current.secret,
                    metadata: &current.metadata,
                    version: current.version,
                    expires_at_ms: current.expires_at_ms,
                },
                &head,
                &mut response.headers,
            );
            let Some(update) = update else {
                return Ok(response);
            };
            if current.status == CredentialStatus::Dead || self.state.is_retired() {
                return Ok(response);
            }
            let sealed = self.codec.seal(&self.id, &update.secret).map_err(storage)?;
            let outcome = self
                .store
                .credentials()
                .refresh_many(vec![CredentialRefresh {
                    id: self.id.clone(),
                    expected_version: current.version,
                    secret: sealed,
                    metadata: update.metadata.clone(),
                    expires_at_ms: update.expires_at_ms,
                }])
                .await
                .map_err(storage)?;
            if outcome.first() == Some(&CasOutcome::Applied) {
                let version = current.version + 1;
                self.state.publish_if_newer(Arc::new(CredentialVersion {
                    version,
                    secret: update.secret,
                    metadata: update.metadata.unwrap_or_else(|| current.metadata.clone()),
                    expires_at_ms: update.expires_at_ms,
                    status: CredentialStatus::Active,
                    status_reason: None,
                }));
                let notification = serde_json::to_vec(&Invalidation::CredentialChanged {
                    credential_id: self.id.clone(),
                    version,
                })
                .map_err(storage)?;
                let _ = self
                    .cache
                    .publish(keys::INVALIDATION_TOPIC, notification)
                    .await;
            }
            Ok(response)
        })
    }
    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        self.inner.connect(request)
    }
}
impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
    pub(crate) fn bind_response_refresh(&self, data: &mut CoreData) {
        for credential in data.credentials.values_mut() {
            let Some(provider) = data.providers.get(&credential.provider_id) else {
                continue;
            };
            if provider.channel.credential_refresh().is_none() {
                continue;
            }
            let credential = Arc::make_mut(credential);
            credential.client = Arc::new(ResponseRefresh {
                inner: credential.client.clone(),
                channel: provider.channel.clone(),
                id: credential.id.clone(),
                provider_id: credential.provider_id.clone(),
                auth_kind: credential.auth_kind.clone(),
                state: credential.state.clone(),
                store: self.store().clone(),
                codec: self.secret_codec().clone(),
                cache: self.cache().clone(),
            });
        }
    }
}
