//! Where a provider's credential comes from and how it is stored.
//!
//! A credential starts in the configuration (pasted) or from a sign-in, and
//! lives in the store once anything has changed it. Every change is a
//! compare-and-exchange on the version it was read as; `renew` is what
//! changes it on the upstream's behalf.

use std::sync::Arc;

use gproxy_client::OutboundClient;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{
        CapabilityError, CapabilityFuture, CasResult, StateWrite, UpstreamConnection, Version,
    },
    connection::Bytes,
};
use gproxy_upstream::{CredentialUpdate, CredentialView, ProviderView};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    engine::{Inner, Refusal},
    renew::{Renew, Unrenewed},
    store::storage,
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Record {
    /// The pasted secret this credential descends from, `None` after a
    /// sign-in. A stored credential whose origin is not what the
    /// configuration says now is ignored: the operator changed the key.
    pub origin: Option<Value>,
    pub kind: String,
    pub secret: Value,
    pub metadata: Value,
    pub expires_at_ms: Option<i64>,
    /// Counts the changes to this credential; `CredentialView::version`.
    pub version: i64,
    /// Why the upstream definitively refused to renew it. A dead credential
    /// is neither sent nor renewed again; a sign-in or another pasted secret
    /// replaces it.
    pub dead: Option<String>,
}

/// A credential as read, with the store version to replace it under.
#[derive(Clone)]
pub(crate) struct Held {
    pub record: Record,
    pub stored: Option<Version>,
}

fn key(provider: &str) -> String {
    format!("credential/{provider}")
}

impl Record {
    pub fn view<'a>(&'a self, provider: &'a str) -> CredentialView<'a> {
        CredentialView {
            id: provider,
            provider_id: provider,
            auth_kind: &self.kind,
            secret: &self.secret,
            metadata: &self.metadata,
            version: self.version,
            expires_at_ms: self.expires_at_ms,
        }
    }
}

impl Inner {
    pub fn provider_view(&self, provider: usize) -> ProviderView<'_> {
        let provider = &self.providers[provider];
        ProviderView {
            id: &provider.name,
            channel: provider.channel.id(),
            base_url: provider.base_url.as_deref(),
            config: &provider.config,
        }
    }

    /// The credential as it stands, without renewing it.
    pub async fn stored_credential(
        &self,
        provider: usize,
    ) -> Result<Option<Held>, CapabilityError> {
        let provider = &self.providers[provider];
        let origin = provider.pasted.as_ref().map(|pasted| &pasted.secret);
        let entry = self.store.get(&key(&provider.name)).await?;
        let stored = entry.as_ref().map(|entry| entry.version.clone());
        if let Some(entry) = entry {
            let record: Record = serde_json::from_slice(&entry.payload)
                .map_err(|error| storage(format!("stored credential is unreadable: {error}")))?;
            if record.origin.as_ref() == origin {
                return Ok(Some(Held { record, stored }));
            }
        }
        // A stored credential of another origin is shadowed, and replaced by
        // the first change to the pasted one.
        Ok(provider.pasted.as_ref().map(|pasted| Held {
            record: Record {
                origin: Some(pasted.secret.clone()),
                kind: pasted.kind.clone(),
                secret: pasted.secret.clone(),
                metadata: Value::Object(Default::default()),
                expires_at_ms: provider.channel.initial_credential_expiry(&pasted.secret),
                version: 0,
                dead: None,
            },
            stored,
        }))
    }

    /// Replace the credential `held` was read as. `None` when another writer
    /// got there first.
    pub async fn replace_credential(
        &self,
        provider: usize,
        held: &Held,
        record: &Record,
    ) -> Result<Option<Held>, CapabilityError> {
        let name = &self.providers[provider].name;
        let payload = serde_json::to_vec(record).expect("a credential record serializes");
        let written = self
            .store
            .compare_exchange(
                &key(name),
                held.stored.clone(),
                Some(StateWrite {
                    payload: Bytes::from(payload),
                    expires_at: None,
                }),
            )
            .await?;
        Ok(match written {
            CasResult::Applied(version) => Some(Held {
                record: record.clone(),
                stored: version,
            }),
            CasResult::Conflict => None,
        })
    }

    pub fn renewed(held: &Held, update: CredentialUpdate) -> Record {
        Record {
            origin: held.record.origin.clone(),
            kind: held.record.kind.clone(),
            secret: update.secret,
            metadata: update
                .metadata
                .unwrap_or_else(|| held.record.metadata.clone()),
            expires_at_ms: update.expires_at_ms,
            version: held.record.version + 1,
            dead: None,
        }
    }

    /// The credential to send with, renewed first when it is about to expire.
    /// A renewal that fails for a transient reason leaves the current
    /// credential to be tried.
    pub async fn credential(&self, provider: usize) -> Result<Held, Refusal> {
        let held = self
            .stored_credential(provider)
            .await?
            .ok_or_else(|| self.not_signed_in(provider))?;
        if let Some(reason) = &held.record.dead {
            return Err(self.sign_in_again(provider, reason));
        }
        if !self.due(provider, &held) {
            return Ok(held);
        }
        match self.renew(provider, &held, Renew::IfDue).await {
            Ok(renewed) => Ok(renewed),
            Err(Unrenewed::Rejected(reason)) => Err(self.sign_in_again(provider, &reason)),
            Err(_) => Ok(held),
        }
    }

    fn sign_in_again(&self, provider: usize, reason: &str) -> Refusal {
        let provider = &self.providers[provider];
        let remedy = match provider.pasted {
            Some(_) => "replace it in the configuration",
            None => "sign in again",
        };
        Refusal::new(
            http::StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "the credential of provider `{}` can no longer be renewed ({reason}); {remedy}",
                provider.name
            ),
        )
    }

    fn not_signed_in(&self, provider: usize) -> Refusal {
        Refusal::new(
            http::StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "provider `{}` has no credential yet; sign in through /lite/login/start",
                self.providers[provider].name
            ),
        )
    }
}

/// The outbound client of one send, watching responses for a credential the
/// upstream rotated in a header. The rotation is stored before the response
/// is handed on; a lost exchange keeps the other writer's credential.
pub(crate) struct RotatingClient {
    pub inner: Arc<Inner>,
    pub provider: usize,
    pub held: Held,
}

impl OutboundClient for RotatingClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let engine = &self.inner;
            let Some(refresher) = engine.providers[self.provider].channel.credential_refresh()
            else {
                return engine.client.send(request).await;
            };
            let mut head = http::Request::new(());
            *head.method_mut() = request.method().clone();
            *head.uri_mut() = request.uri().clone();
            *head.headers_mut() = request.headers().clone();
            let mut response = engine.client.send(request).await?;
            let view = self.held.record.view(&engine.providers[self.provider].name);
            if let Some(update) = refresher.response_update(view, &head, &mut response.headers) {
                let record = Inner::renewed(&self.held, update);
                engine
                    .replace_credential(self.provider, &self.held, &record)
                    .await?;
            }
            Ok(response)
        })
    }

    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        self.inner.client.connect(request)
    }
}
