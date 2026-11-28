//! Turning a person's browser session into a credential row.
//!
//! Three flows, one shape: a channel knows how to talk to its upstream, this
//! module knows everything that is not the upstream's business — the PKCE
//! verifier, the CSRF state, where the pending session lives, and how what
//! comes back becomes a durable, sealed credential.
//!
//! What the SDK owns, and a caller therefore never has to:
//!
//! - **PKCE.** The verifier is minted here (`gproxy_upstream::login`), never
//!   sent, and only its S256 digest reaches the upstream.
//! - **CSRF state.** Minted here and compared here. A callback whose state does
//!   not match the one this session was started with is refused and the session
//!   is destroyed, so a replay cannot be retried into success.
//! - **The session.** It lives in the shared cache under its own TTL, which is
//!   what lets any instance of a deployment complete a login another started,
//!   and what makes an abandoned login cost nothing. It also carries the
//!   channel's own `provider_state` from the first step to the last — in both
//!   flows, and without reading it — which is the only way a secret a login
//!   minted for itself can reach the step that needs it.
//! - **Sealing.** The acquired secret is sealed with the configured codec
//!   before the insert statement is built.
//!
//! What the caller owns: the polling cadence of a device login. `device_poll`
//! performs exactly one step and returns what the upstream said, including the
//! interval to wait before asking again. Nothing here sleeps or loops.

mod persist;
mod session;

use std::sync::Arc;

use gproxy_core::ProviderData;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::upstream::provider;
use gproxy_upstream::{
    AuthorizationCode, AuthorizationRequest, DevicePoll, LoginContext, LoginMode, UpstreamKind,
};

use crate::{
    SdkError, SdkResult,
    dto::{
        AuthCodeComplete, AuthCodeStart, AuthCodeStarted, CredentialCreated, CredentialOwner,
        DevicePollOutcome, DeviceStart, DeviceStarted,
    },
    handle::Inner,
    manage::{Manage, Writer},
};
use session::LoginSession;

/// The login side of a handle, one method per step of each flow.
pub struct Login<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

impl<'a, C> Login<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }

    /// The provider as the active snapshot has it, which is also what decides
    /// that it exists and is enabled. A login against a provider this instance
    /// is not serving would acquire a credential nothing could use.
    fn provider(&self, provider_id: &str) -> SdkResult<Arc<ProviderData>> {
        let provider_id = provider_id.trim();
        if provider_id.is_empty() {
            return Err(SdkError::invalid("providerId must not be blank"));
        }
        self.inner
            .core
            .snapshot()
            .providers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| SdkError::not_found("provider", provider_id))
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Login<'_, C> {
    /// Begin a browser redirect login: mint the PKCE pair and the state, ask
    /// the channel for the authorize URL, and park everything the completion
    /// will need.
    pub async fn authcode_start(&self, request: AuthCodeStart) -> SdkResult<AuthCodeStarted> {
        let provider = self.provider(&request.provider_id)?;
        let Some(flow) = provider.upstream().oauth_authorization_code() else {
            return Err(SdkError::Unsupported(
                "this channel has no authorization code login",
            ));
        };
        let variant = checked_variant(
            provider.upstream(),
            request.variant.as_deref(),
            LoginMode::AuthorizationCode,
        )?;
        let pkce = gproxy_upstream::login::pkce(flow.pkce_verifier_bytes())
            .map_err(|_| SdkError::invalid("secure randomness is unavailable"))?;
        let state = crate::ids::random_id();
        let client = self.inner.core.provider_client(&provider.entity.id).await?;
        // An empty redirect leaves the choice to the channel, which is what a
        // CLI-shaped upstream with a registered loopback address requires. The
        // answer echoes whatever it settled on, and that is what is stored: the
        // token exchange must repeat it verbatim.
        let started = flow
            .authorize(
                LoginContext {
                    provider: provider.view(),
                    client: client.as_ref(),
                    variant: variant.as_deref(),
                },
                AuthorizationRequest {
                    redirect_uri: request.redirect_uri.as_deref().unwrap_or_default(),
                    state: &state,
                    code_challenge: &pkce.challenge,
                },
            )
            .await?;
        let login_session_id = crate::ids::random_id();
        session::store(
            &self.inner.cache,
            &login_session_id,
            &LoginSession::AuthCode {
                provider_id: provider.entity.id.clone(),
                channel: provider.entity.channel.clone(),
                verifier: pkce.verifier,
                state,
                redirect_uri: started.redirect_uri.clone(),
                provider_state: started.provider_state,
                variant,
                label: request.label,
                owner: request.owner,
            },
            session::authcode_ttl(self.inner.login_ttl),
        )
        .await?;
        Ok(AuthCodeStarted {
            login_session_id,
            authorize_url: started.authorize_url,
            redirect_uri: started.redirect_uri,
        })
    }

    /// Finish a browser redirect login. The caller hands back either the whole
    /// callback URL or the code it already parsed out of one — never both,
    /// because the two could disagree and there is no principled winner.
    pub async fn authcode_complete(
        &self,
        request: AuthCodeComplete,
    ) -> SdkResult<CredentialCreated> {
        let session_id = request.login_session_id.clone();
        let loaded = session::load(&self.inner.cache, &session_id).await?;
        let LoginSession::AuthCode {
            provider_id,
            channel,
            verifier,
            state: expected_state,
            redirect_uri,
            provider_state,
            variant,
            label,
            owner,
        } = loaded
        else {
            return Err(SdkError::invalid(
                "this login session is not an authorization code login",
            ));
        };
        let (code, state) = gproxy_upstream::login::authorization_code(
            request.callback_url.as_deref(),
            request.code.as_deref(),
            request.state.as_deref(),
        )
        .map_err(|error| SdkError::invalid(error.0))?;
        if let Some(state) = state
            && state != expected_state
        {
            // Either a stale callback or a forged one; both are finished with
            // this session, so nothing is left for a second attempt to use.
            session::delete(&self.inner.cache, &session_id).await?;
            return Err(SdkError::invalid("authorization state mismatch"));
        }
        let provider = self.provider(&provider_id)?;
        expect_channel(&provider.entity, &channel)?;
        let Some(flow) = provider.upstream().oauth_authorization_code() else {
            return Err(SdkError::Unsupported(
                "this channel has no authorization code login",
            ));
        };
        let client = self.inner.core.provider_client(&provider.entity.id).await?;
        let acquired = flow
            .exchange(
                LoginContext {
                    provider: provider.view(),
                    client: client.as_ref(),
                    variant: variant.as_deref(),
                },
                AuthorizationCode {
                    code: &code,
                    redirect_uri: &redirect_uri,
                    code_verifier: &verifier,
                    state: &expected_state,
                    // Whatever `authorize` left for this moment, unread by
                    // anything in between.
                    provider_state: &provider_state,
                },
            )
            .await?;
        // Only now: a refused exchange leaves the session usable, so a caller
        // that mistyped a code or hit a flaky upstream can simply try again.
        session::delete(&self.inner.cache, &session_id).await?;
        self.created(&provider, persist::OAUTH, label, owner, acquired.into())
            .await
    }

    /// Begin a device login. What comes back is what to show the person; the
    /// authorization itself is parked for the poll.
    pub async fn device_start(&self, request: DeviceStart) -> SdkResult<DeviceStarted> {
        let provider = self.provider(&request.provider_id)?;
        let Some(flow) = provider.upstream().oauth_device_code() else {
            return Err(SdkError::Unsupported(
                "this channel has no device code login",
            ));
        };
        let variant = checked_variant(
            provider.upstream(),
            request.variant.as_deref(),
            LoginMode::DeviceCode,
        )?;
        let client = self.inner.core.provider_client(&provider.entity.id).await?;
        let authorization = flow
            .start(LoginContext {
                provider: provider.view(),
                client: client.as_ref(),
                variant: variant.as_deref(),
            })
            .await?;
        let login_session_id = crate::ids::random_id();
        let started = DeviceStarted {
            login_session_id: login_session_id.clone(),
            user_code: authorization.user_code.clone(),
            verification_uri: authorization.verification_uri.clone(),
            verification_uri_complete: authorization.verification_uri_complete.clone(),
            interval_secs: authorization.interval_secs,
            expires_at_ms: authorization.expires_at_ms,
        };
        let ttl = session::device_ttl(self.inner.login_ttl, &authorization);
        session::store(
            &self.inner.cache,
            &login_session_id,
            &LoginSession::Device {
                provider_id: provider.entity.id.clone(),
                channel: provider.entity.channel.clone(),
                interval_secs: authorization.interval_secs,
                authorization,
                variant,
                label: request.label,
                owner: request.owner,
            },
            ttl,
        )
        .await?;
        Ok(started)
    }

    /// One polling step, and only one. A `slow_down` rewrites the stored
    /// cadence and is reported as `Pending` with the new interval, because to
    /// the caller they mean the same thing: wait this long, then ask again.
    pub async fn device_poll(&self, login_session_id: &str) -> SdkResult<DevicePollOutcome> {
        let loaded = session::load(&self.inner.cache, login_session_id).await?;
        let LoginSession::Device {
            provider_id,
            channel,
            authorization,
            variant,
            label,
            owner,
            interval_secs,
        } = loaded
        else {
            return Err(SdkError::invalid(
                "this login session is not a device code login",
            ));
        };
        let provider = self.provider(&provider_id)?;
        expect_channel(&provider.entity, &channel)?;
        let Some(flow) = provider.upstream().oauth_device_code() else {
            return Err(SdkError::Unsupported(
                "this channel has no device code login",
            ));
        };
        let client = self.inner.core.provider_client(&provider.entity.id).await?;
        let poll = flow
            .poll(
                LoginContext {
                    provider: provider.view(),
                    client: client.as_ref(),
                    variant: variant.as_deref(),
                },
                &authorization,
            )
            .await?;
        match poll {
            DevicePoll::Pending => Ok(DevicePollOutcome::Pending { interval_secs }),
            DevicePoll::SlowDown {
                interval_secs: raised,
            } => {
                let ttl = session::device_ttl(self.inner.login_ttl, &authorization);
                session::store(
                    &self.inner.cache,
                    login_session_id,
                    &LoginSession::Device {
                        provider_id,
                        channel,
                        authorization,
                        variant,
                        label,
                        owner,
                        interval_secs: raised,
                    },
                    ttl,
                )
                .await?;
                Ok(DevicePollOutcome::Pending {
                    interval_secs: raised,
                })
            }
            DevicePoll::Ready(credential) => {
                session::delete(&self.inner.cache, login_session_id).await?;
                let created = self
                    .created(&provider, persist::OAUTH, label, owner, credential.into())
                    .await?;
                Ok(DevicePollOutcome::Ready {
                    credential_id: created.credential_id,
                })
            }
            // Both are final. Keeping the session would only let a caller poll
            // an authorization the upstream has already closed.
            DevicePoll::Denied => {
                session::delete(&self.inner.cache, login_session_id).await?;
                Ok(DevicePollOutcome::Denied)
            }
            DevicePoll::Expired => {
                session::delete(&self.inner.cache, login_session_id).await?;
                Ok(DevicePollOutcome::Expired)
            }
        }
    }

    async fn created(
        &self,
        provider: &ProviderData,
        auth_kind: &str,
        label: Option<String>,
        owner: CredentialOwner,
        acquired: gproxy_upstream::AcquiredCredential,
    ) -> SdkResult<CredentialCreated> {
        let credential_id = persist::credential(
            self.writer(),
            &provider.entity,
            // A channel names its product; a tool server is known by its provider name.
            match provider.channel() {
                Some(channel) => channel.descriptor().display_name,
                None => &provider.entity.name,
            },
            auth_kind,
            label,
            owner,
            acquired,
        )
        .await?;
        Ok(CredentialCreated { credential_id })
    }

    /// The same write primitive the management families use: one revision
    /// commit, a reload, then the peer notification.
    fn writer(&self) -> Writer<'_, C> {
        Manage::new(self.inner).writer()
    }
}

/// That the provider still speaks the channel the login was started against. A
/// provider re-pointed mid-login would otherwise have a code minted by one
/// upstream exchanged against another.
/// The variant a login request named, if the kind declares it for this flow.
/// Blank counts as absent; anything else undeclared is refused rather than
/// passed to a channel that would not know it.
fn checked_variant(
    kind: &dyn UpstreamKind,
    requested: Option<&str>,
    mode: LoginMode,
) -> SdkResult<Option<String>> {
    let Some(requested) = requested.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    match kind
        .login_variants()
        .iter()
        .find(|variant| variant.id == requested)
    {
        Some(variant) if variant.mode == mode => Ok(Some(variant.id.to_owned())),
        Some(_) => Err(SdkError::invalid(format!(
            "login variant `{requested}` belongs to another login flow"
        ))),
        None => Err(SdkError::invalid(format!(
            "this channel has no login variant `{requested}`"
        ))),
    }
}

fn expect_channel(provider: &provider::Model, expected: &str) -> SdkResult<()> {
    if provider.channel != expected {
        return Err(SdkError::invalid(format!(
            "provider `{}` now uses channel `{}`, not the `{expected}` this login started with",
            provider.id, provider.channel
        )));
    }
    Ok(())
}
