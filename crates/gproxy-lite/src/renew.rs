//! Renewing a credential: before a request that finds it due, after a `401`,
//! and on the host's schedule ([`Engine::renew_due`]).
//!
//! One renewal per credential at a time, across instances: some upstreams
//! revoke a whole token family when a refresh token is presented twice, and
//! the compare-and-exchange on the write-back only decides whose result is
//! kept. The renewer holds a lease, a store entry created only when absent
//! that expires on its own; everyone else waits for the credential's version
//! to move and uses what the holder stored. This is the full engine's
//! `refresh_credential`, on the key-value store instead of its cache.

use std::time::{Duration, SystemTime};

use gproxy_protocol::{
    capability::{CapabilityError, CasResult, StateWrite, Version},
    connection::Bytes,
};
use gproxy_upstream::{CredentialContext, RefreshReason, UpstreamError};
use serde::Serialize;

use crate::{
    Engine,
    credential::{Held, Record},
    engine::Inner,
};

/// A renewal that runs longer than this lost its lease; another may start.
/// The full engine's: a token endpoint answers in a second or two, and every
/// platform gives a function at least this long.
const LEASE_TTL: Duration = Duration::from_secs(30);
/// Waiting for the holder: the full engine's patience, which fits the
/// shortest platform budget (a scheduled Netlify function, 30 seconds).
const LEASE_WAIT: Duration = Duration::from_secs(10);
/// Coarser than the full engine's 100 ms: every poll is a billed round trip
/// to the platform's store, not a read of local memory.
const LEASE_POLL: Duration = Duration::from_millis(250);

#[cfg(not(target_arch = "wasm32"))]
async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(target_arch = "wasm32")]
async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

/// What the caller of a renewal wants.
#[derive(Clone, Copy)]
pub(crate) enum Renew {
    /// Renew if the channel still finds the credential due.
    IfDue,
    /// The upstream answered `401` to the credential as read.
    Unauthorized,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Unrenewed {
    /// The upstream's definitive refusal: only a new credential helps.
    #[error("the upstream refused to renew it ({0}); it has to be replaced")]
    Rejected(String),
    #[error("another renewal of it did not finish in time")]
    Contended,
    #[error(transparent)]
    Failed(#[from] UpstreamError),
}

impl From<CapabilityError> for Unrenewed {
    fn from(error: CapabilityError) -> Self {
        Self::Failed(error.into())
    }
}

fn alive(held: Held) -> Result<Held, Unrenewed> {
    match held.record.dead.clone() {
        Some(reason) => Err(Unrenewed::Rejected(reason)),
        None => Ok(held),
    }
}

impl Inner {
    /// Whether the channel can renew this credential and finds it due now.
    pub fn due(&self, provider: usize, held: &Held) -> bool {
        let provider = &self.providers[provider];
        let view = held.record.view(&provider.name);
        provider
            .channel
            .credential_refresh()
            .is_some_and(|refresher| {
                refresher.supports(&view)
                    && refresher
                        .proactive_refresh_reason(view, gproxy_convert::now_ms())
                        .is_some()
            })
    }

    fn lease_key(&self, provider: usize) -> String {
        format!("lease/credential/{}", self.providers[provider].name)
    }

    /// The lease, unless someone holds one that has not expired.
    async fn take_lease(&self, provider: usize) -> Result<Option<Version>, CapabilityError> {
        let taken = self
            .store
            .compare_exchange(
                &self.lease_key(provider),
                None,
                Some(StateWrite {
                    payload: Bytes::new(),
                    expires_at: Some(
                        SystemTime::UNIX_EPOCH
                            + Duration::from_millis(gproxy_convert::now_ms() as u64)
                            + LEASE_TTL,
                    ),
                }),
            )
            .await?;
        Ok(match taken {
            CasResult::Applied(version) => version,
            CasResult::Conflict => None,
        })
    }

    /// Renew the credential `held` was read as, or return the credential
    /// another renewal stored in the meantime.
    pub async fn renew(
        &self,
        provider: usize,
        held: &Held,
        mode: Renew,
    ) -> Result<Held, Unrenewed> {
        let waiting_since = gproxy_convert::now_ms();
        let lease = loop {
            if let Some(lease) = self.take_lease(provider).await? {
                break lease;
            }
            if let Some(current) = self.stored_credential(provider).await?
                && current.record.version != held.record.version
            {
                return alive(current);
            }
            if gproxy_convert::now_ms() - waiting_since >= LEASE_WAIT.as_millis() as i64 {
                return Err(Unrenewed::Contended);
            }
            sleep(LEASE_POLL).await;
        };
        let renewed = self.renew_under_lease(provider, held, mode).await;
        // Left behind, it expires on its own; the renewal stands either way.
        let _ = self
            .store
            .compare_exchange(&self.lease_key(provider), Some(lease), None)
            .await;
        renewed
    }

    async fn renew_under_lease(
        &self,
        provider: usize,
        held: &Held,
        mode: Renew,
    ) -> Result<Held, Unrenewed> {
        let name = &self.providers[provider].name;
        let refresher = self.providers[provider]
            .channel
            .credential_refresh()
            .ok_or(UpstreamError::UnsupportedService)?;
        // Whoever held the lease before may have renewed it already.
        let current = self.winner(provider).await?;
        if current.record.version != held.record.version {
            return Ok(current);
        }
        let view = current.record.view(name);
        let reason = match mode {
            Renew::IfDue => {
                match refresher.proactive_refresh_reason(view, gproxy_convert::now_ms()) {
                    Some(reason) => reason,
                    None => return Ok(current),
                }
            }
            Renew::Unauthorized => RefreshReason::UnauthorizedRecovery,
        };
        if let Some(reason) = self.rejection(provider, current.record.version) {
            return Err(Unrenewed::Rejected(reason));
        }
        let context = CredentialContext {
            provider: self.provider_view(provider),
            credential: view,
            client: self.client.as_ref(),
        };
        match refresher.refresh_with_reason(context, reason).await {
            Ok(update) => {
                let record = Self::renewed(&current, update);
                match self.replace_credential(provider, &current, &record).await? {
                    Some(stored) => Ok(stored),
                    // A sign-in replaced it meanwhile.
                    None => self.winner(provider).await,
                }
            }
            Err(UpstreamError::RefreshRejected(reason)) => {
                if refresher.persist_rejection(&reason) {
                    let dead = Record {
                        dead: Some(reason.clone()),
                        version: current.record.version + 1,
                        ..current.record.clone()
                    };
                    if self
                        .replace_credential(provider, &current, &dead)
                        .await?
                        .is_none()
                    {
                        return self.winner(provider).await;
                    }
                } else {
                    self.rejections
                        .lock()
                        .expect("rejections lock")
                        .insert(provider, (current.record.version, reason.clone()));
                }
                Err(Unrenewed::Rejected(reason))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The stored credential, which a renewal that lost defers to.
    async fn winner(&self, provider: usize) -> Result<Held, Unrenewed> {
        let stored = self
            .stored_credential(provider)
            .await?
            .ok_or(UpstreamError::InvalidCredential)?;
        alive(stored)
    }

    /// A refusal the channel does not want stored, remembered by this
    /// instance for the version it was given to.
    fn rejection(&self, provider: usize, version: i64) -> Option<String> {
        self.rejections
            .lock()
            .expect("rejections lock")
            .get(&provider)
            .filter(|(rejected, _)| *rejected == version)
            .map(|(_, reason)| reason.clone())
    }

    pub async fn renew_due(&self) -> Vec<Renewal> {
        let mut renewals = Vec::new();
        for (index, provider) in self.providers.iter().enumerate() {
            if let Some(outcome) = self.renew_if_due(index).await {
                renewals.push(Renewal {
                    provider: provider.name.clone(),
                    outcome,
                });
            }
        }
        renewals
    }

    /// `None` for a provider whose credential cannot be renewed.
    async fn renew_if_due(&self, provider: usize) -> Option<Outcome> {
        let refresher = self.providers[provider].channel.credential_refresh()?;
        let held = match self.stored_credential(provider).await {
            Ok(Some(held)) => held,
            Ok(None) => return Some(Outcome::NoCredential),
            Err(error) => return Some(Outcome::failed(error)),
        };
        if !refresher.supports(&held.record.view(&self.providers[provider].name)) {
            return None;
        }
        if let Some(reason) = held.record.dead.clone() {
            return Some(Outcome::failed(Unrenewed::Rejected(reason)));
        }
        if !self.due(provider, &held) {
            return Some(Outcome::NotDue);
        }
        Some(match self.renew(provider, &held, Renew::IfDue).await {
            Ok(renewed) if renewed.record.version != held.record.version => Outcome::Renewed,
            Ok(_) => Outcome::NotDue,
            Err(error) => Outcome::failed(error),
        })
    }
}

/// What [`Engine::renew_due`] did for one provider.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Renewal {
    pub provider: String,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Renewed,
    NotDue,
    /// Nobody has signed in yet.
    NoCredential,
    Failed {
        reason: String,
    },
}

impl Outcome {
    fn failed(error: impl std::fmt::Display) -> Self {
        Self::Failed {
            reason: error.to_string(),
        }
    }
}

impl Engine {
    /// Renew every credential that is due, without a request to use it: what
    /// the platform's scheduler calls, so a sign-in outlives an idle period.
    /// Providers whose credential cannot be renewed (an API key) are left
    /// out; one that fails does not stop the others.
    ///
    /// The full engine's timer renews only the channels that ask for it
    /// (`CredentialRefresh::keepalive`) and leaves tokens to the next request.
    /// Here every renewable credential is kept alive: a deployment that
    /// nothing calls for weeks is the ordinary case, not the exception.
    pub async fn renew_due(&self) -> Vec<Renewal> {
        self.0.renew_due().await
    }
}
