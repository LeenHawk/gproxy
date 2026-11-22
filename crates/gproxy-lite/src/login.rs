//! Signing in to a channel that takes no pasted key, without a browser
//! redirect ever landing on the deployment: `start` hands back the
//! authorization URL or the device code, the person authorizes, and
//! `complete` takes the callback URL (or the bare code) they copied, or polls
//! the device authorization once. The convention, PKCE and callback parsing
//! are the full gateway's (`gproxy_upstream::login`).
//!
//! The pending login is a store entry with its own deadline, so any instance
//! can finish what another started.

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime},
};

use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CasResult, StateWrite, Version},
    connection::Bytes,
};
use gproxy_upstream::{
    AcquiredCredential, AuthorizationCode, AuthorizationRequest, DeviceAuthorization, DevicePoll,
    LoginContext, LoginMode,
};
use http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    credential::Record,
    engine::{Inner, Refusal, json_response},
};

const AUTHORIZATION_CODE_TTL: Duration = Duration::from_secs(10 * 60);
const DEVICE_CODE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
enum Session {
    AuthorizationCode {
        provider: String,
        /// Never sent to the upstream until the exchange; only its digest was.
        verifier: String,
        /// Minted here; the callback must echo it.
        state: String,
        /// What the channel asked the upstream to redirect to, repeated
        /// verbatim in the exchange.
        redirect_uri: String,
        provider_state: BTreeMap<String, Value>,
    },
    DeviceCode {
        provider: String,
        authorization: DeviceAuthorization,
        /// The polling cadence, which a `slow_down` answer raises.
        interval_secs: u64,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    provider: String,
    #[serde(default)]
    mode: Option<LoginMode>,
    #[serde(default)]
    redirect_uri: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Complete {
    session: String,
    #[serde(default)]
    callback_url: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    state: Option<String>,
}

fn key(session: &str) -> String {
    format!("login/{session}")
}

fn body<T: for<'de> Deserialize<'de>>(bytes: &Bytes) -> Result<T, Refusal> {
    serde_json::from_slice(bytes).map_err(|error| Refusal::bad_request(error.to_string()))
}

fn ok(body: Value) -> WireResponse<HttpBody> {
    json_response(StatusCode::OK, &body)
}

pub(crate) async fn serve(
    inner: &Arc<Inner>,
    method: &Method,
    action: &str,
    request: &Bytes,
) -> Result<WireResponse<HttpBody>, Refusal> {
    match (method.as_str(), action) {
        ("GET", "status") => inner.status().await,
        ("POST", "renew") => Ok(ok(json!({ "renewals": inner.renew_due().await }))),
        ("POST", "login/start") => inner.start(body(request)?).await,
        ("POST", "login/complete") => inner.complete(body(request)?).await,
        _ => Err(Refusal::new(StatusCode::NOT_FOUND, "no such path")),
    }
}

impl Inner {
    fn provider_named(&self, name: &str) -> Result<usize, Refusal> {
        self.providers
            .iter()
            .position(|provider| provider.name == name)
            .ok_or_else(|| Refusal::new(StatusCode::NOT_FOUND, format!("no provider `{name}`")))
    }

    async fn status(&self) -> Result<WireResponse<HttpBody>, Refusal> {
        let mut providers = Vec::with_capacity(self.providers.len());
        for (index, provider) in self.providers.iter().enumerate() {
            let held = self.stored_credential(index).await?;
            providers.push(json!({
                "name": provider.name,
                "channel": provider.channel.id(),
                "login_modes": provider.channel.descriptor().login_modes,
                "credential": match &held {
                    None => "none",
                    Some(held) if held.record.dead.is_some() => "dead",
                    Some(held) if held.record.origin.is_some() => "configured",
                    Some(_) => "signed_in",
                },
                "reason": held.as_ref().and_then(|held| held.record.dead.clone()),
                "expires_at_ms": held.as_ref().and_then(|held| held.record.expires_at_ms),
                "metadata": held.map(|held| held.record.metadata),
            }));
        }
        Ok(ok(json!({ "providers": providers })))
    }

    async fn park(&self, session: &Session, ttl: Duration) -> Result<String, Refusal> {
        let id = gproxy_convert::random_id();
        let payload = serde_json::to_vec(session).expect("a login session serializes");
        self.store
            .compare_exchange(
                &key(&id),
                None,
                Some(StateWrite {
                    payload: Bytes::from(payload),
                    expires_at: Some(
                        SystemTime::UNIX_EPOCH
                            + Duration::from_millis(gproxy_convert::now_ms() as u64)
                            + ttl,
                    ),
                }),
            )
            .await?;
        Ok(id)
    }

    async fn start(&self, request: Start) -> Result<WireResponse<HttpBody>, Refusal> {
        let index = self.provider_named(&request.provider)?;
        let provider = &self.providers[index];
        if provider.pasted.is_some() {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "this provider's credential is in the configuration",
            ));
        }
        let context = LoginContext {
            provider: self.provider_view(index),
            client: self.client.as_ref(),
        };
        let code_flow = provider.channel.oauth_authorization_code();
        let device_flow = provider.channel.oauth_device_code();
        match (request.mode, code_flow, device_flow) {
            (Some(LoginMode::AuthorizationCode) | None, Some(flow), _) => {
                let pkce =
                    gproxy_upstream::login::pkce(flow.pkce_verifier_bytes()).map_err(|_| {
                        Refusal::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "secure randomness is unavailable",
                        )
                    })?;
                let state = gproxy_convert::random_id();
                // An empty redirect leaves the choice to the channel; what it
                // settled on is stored, because the exchange must repeat it.
                let started = flow
                    .authorize(
                        context,
                        AuthorizationRequest {
                            redirect_uri: request.redirect_uri.as_deref().unwrap_or_default(),
                            state: &state,
                            code_challenge: &pkce.challenge,
                        },
                    )
                    .await?;
                let session = self
                    .park(
                        &Session::AuthorizationCode {
                            provider: provider.name.clone(),
                            verifier: pkce.verifier,
                            state,
                            redirect_uri: started.redirect_uri.clone(),
                            provider_state: started.provider_state,
                        },
                        AUTHORIZATION_CODE_TTL,
                    )
                    .await?;
                Ok(ok(json!({
                    "session": session,
                    "mode": LoginMode::AuthorizationCode,
                    "authorize_url": started.authorize_url,
                    "redirect_uri": started.redirect_uri,
                })))
            }
            (Some(LoginMode::DeviceCode) | None, _, Some(flow)) => {
                let authorization = flow.start(context).await?;
                let shown = json!({
                    "mode": LoginMode::DeviceCode,
                    "user_code": authorization.user_code,
                    "verification_uri": authorization.verification_uri,
                    "verification_uri_complete": authorization.verification_uri_complete,
                    "interval_secs": authorization.interval_secs,
                    "expires_at_ms": authorization.expires_at_ms,
                });
                // Polling a code the upstream has forgotten cannot succeed.
                let ttl = authorization.expires_at_ms.map_or(DEVICE_CODE_TTL, |at| {
                    let left = at.saturating_sub(gproxy_convert::now_ms()).max(1000);
                    DEVICE_CODE_TTL.min(Duration::from_millis(left as u64))
                });
                let session = self
                    .park(
                        &Session::DeviceCode {
                            provider: provider.name.clone(),
                            interval_secs: authorization.interval_secs,
                            authorization,
                        },
                        ttl,
                    )
                    .await?;
                let mut shown = shown;
                shown["session"] = session.into();
                Ok(ok(shown))
            }
            _ => Err(Refusal::bad_request(
                "this provider's channel does not sign in that way",
            )),
        }
    }

    async fn complete(&self, request: Complete) -> Result<WireResponse<HttpBody>, Refusal> {
        let expired = || Refusal::new(StatusCode::GONE, "this login has expired; start again");
        let entry = self
            .store
            .get(&key(&request.session))
            .await?
            .ok_or_else(expired)?;
        let session: Session = serde_json::from_slice(&entry.payload).map_err(|_| expired())?;
        let forget = |version: Version| async {
            self.store
                .compare_exchange(&key(&request.session), Some(version), None)
                .await
        };
        match session {
            Session::AuthorizationCode {
                provider,
                verifier,
                state: expected_state,
                redirect_uri,
                provider_state,
            } => {
                let (code, state) = gproxy_upstream::login::authorization_code(
                    request.callback_url.as_deref(),
                    request.code.as_deref(),
                    request.state.as_deref(),
                )
                .map_err(|error| Refusal::bad_request(error.0))?;
                if state.is_some_and(|state| state != expected_state) {
                    // A stale callback or a forged one; nothing is left for a
                    // second attempt to use.
                    forget(entry.version).await?;
                    return Err(Refusal::bad_request("authorization state mismatch"));
                }
                let index = self.provider_named(&provider)?;
                let flow = self.providers[index]
                    .channel
                    .oauth_authorization_code()
                    .expect("the session was started on this flow");
                let credential = flow
                    .exchange(
                        LoginContext {
                            provider: self.provider_view(index),
                            client: self.client.as_ref(),
                        },
                        AuthorizationCode {
                            code: &code,
                            redirect_uri: &redirect_uri,
                            code_verifier: &verifier,
                            state: &expected_state,
                            provider_state: &provider_state,
                        },
                    )
                    .await?;
                // Only now: a refused exchange leaves the session to retry.
                forget(entry.version).await?;
                self.signed_in(index, credential.into()).await?;
                Ok(ok(json!({ "status": "ready" })))
            }
            Session::DeviceCode {
                provider,
                authorization,
                interval_secs,
            } => {
                let index = self.provider_named(&provider)?;
                let flow = self.providers[index]
                    .channel
                    .oauth_device_code()
                    .expect("the session was started on this flow");
                let poll = flow
                    .poll(
                        LoginContext {
                            provider: self.provider_view(index),
                            client: self.client.as_ref(),
                        },
                        &authorization,
                    )
                    .await?;
                let pending = |interval_secs: u64| {
                    ok(json!({ "status": "pending", "interval_secs": interval_secs }))
                };
                match poll {
                    DevicePoll::Pending => Ok(pending(interval_secs)),
                    DevicePoll::SlowDown { interval_secs } => {
                        let slowed = Session::DeviceCode {
                            provider,
                            authorization,
                            interval_secs,
                        };
                        self.store
                            .compare_exchange(
                                &key(&request.session),
                                Some(entry.version),
                                Some(StateWrite {
                                    payload: Bytes::from(
                                        serde_json::to_vec(&slowed)
                                            .expect("a login session serializes"),
                                    ),
                                    expires_at: entry.expires_at,
                                }),
                            )
                            .await?;
                        Ok(pending(interval_secs))
                    }
                    DevicePoll::Ready(credential) => {
                        forget(entry.version).await?;
                        self.signed_in(index, credential.into()).await?;
                        Ok(ok(json!({ "status": "ready" })))
                    }
                    DevicePoll::Denied => {
                        forget(entry.version).await?;
                        Ok(ok(json!({ "status": "denied" })))
                    }
                    DevicePoll::Expired => {
                        forget(entry.version).await?;
                        Ok(ok(json!({ "status": "expired" })))
                    }
                }
            }
        }
    }

    /// A sign-in replaces whatever credential the provider had.
    async fn signed_in(
        &self,
        provider: usize,
        acquired: AcquiredCredential,
    ) -> Result<(), Refusal> {
        let name = &self.providers[provider].name;
        let key = format!("credential/{name}");
        let current = self.store.get(&key).await?;
        let previous = current
            .as_ref()
            .and_then(|entry| serde_json::from_slice::<Record>(&entry.payload).ok())
            .map_or(0, |record| record.version);
        let record = Record {
            origin: None,
            kind: "oauth".to_owned(),
            secret: acquired.secret,
            metadata: acquired.metadata,
            expires_at_ms: acquired.expires_at_ms,
            version: previous + 1,
            dead: None,
        };
        let written = self
            .store
            .compare_exchange(
                &key,
                current.map(|entry| entry.version),
                Some(StateWrite {
                    payload: Bytes::from(
                        serde_json::to_vec(&record).expect("a credential record serializes"),
                    ),
                    expires_at: None,
                }),
            )
            .await?;
        match written {
            CasResult::Applied(_) => Ok(()),
            CasResult::Conflict => Err(Refusal::new(
                StatusCode::CONFLICT,
                "the credential changed while signing in; complete the login again",
            )),
        }
    }
}
