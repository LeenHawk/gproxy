//! The pending half of a login, parked in the shared cache.
//!
//! A browser redirect login is two requests with a person's attention in
//! between, and a device login is a poll loop; both need state that outlives
//! the request that created it. That state lives in the cache rather than in
//! this process, so the instance that finishes a login need not be the one that
//! started it — behind a load balancer it usually is not. It also means a
//! session cannot outlive its window: the cache expires it, and an expired key
//! is indistinguishable from one that was never there, which is exactly the
//! answer a caller should get for either.
//!
//! Nothing durable is written until the login succeeds. Abandoning a login
//! leaves nothing behind but a key that expires on its own.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use gproxy_cache::Cache;
use gproxy_channel::channel::DeviceAuthorization;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{SdkError, SdkResult, builder::LoginTtl, dto::CredentialOwner};

/// Versioned and crate-prefixed: the cache is shared with core's own keys and
/// with whatever the application layer puts there, and a payload shape that
/// changes must not be read back by an older peer as if it had not.
const PREFIX: &str = "gproxy-sdk:v1:login:";

/// A device session whose window has already closed is still worth storing for
/// a moment, so the next poll answers `Expired` from the channel rather than
/// `LoginExpired` from a missing key.
const MIN_TTL: Duration = Duration::from_secs(1);

pub(crate) fn key(session_id: &str) -> String {
    format!("{PREFIX}{session_id}")
}

/// What a login left behind, tagged by flow so one cannot be finished as the
/// other.
#[derive(Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub(crate) enum LoginSession {
    AuthCode {
        provider_id: String,
        /// The channel the provider named when the login started. A provider
        /// re-pointed at another channel mid-login must not have its code
        /// exchanged by whatever channel is there now.
        channel: String,
        /// Never sent to the upstream until the exchange; only its digest was.
        verifier: String,
        /// The CSRF state minted here, which the callback must echo.
        state: String,
        /// What the channel actually asked the upstream to redirect to, which
        /// the token exchange has to repeat verbatim.
        redirect_uri: String,
        /// The channel's own facts from the authorize step, handed back to
        /// `exchange` — the same pocket a device authorization carries, and
        /// secret-bearing for the same reason: a client a login registered for
        /// itself has a secret, and it belongs here rather than on the
        /// credential's published metadata. This session is where it lives and
        /// the only place it does.
        provider_state: BTreeMap<String, Value>,
        label: Option<String>,
        owner: CredentialOwner,
    },
    Device {
        provider_id: String,
        channel: String,
        /// The channel's own handle on the pending authorization, including
        /// the `provider_state` a non-standard device flow needs.
        authorization: DeviceAuthorization,
        label: Option<String>,
        owner: CredentialOwner,
        /// The current polling cadence, which a `slow_down` answer raises.
        /// Kept beside the authorization because the upstream revises it after
        /// the authorization was issued.
        interval_secs: u64,
    },
}

/// How long a session of each flow is worth keeping. A device flow is bounded
/// by the upstream's own window when it named one: polling a user code the
/// upstream has already forgotten cannot succeed, and holding the key longer
/// only delays the honest answer.
pub(crate) fn authcode_ttl(ttl: LoginTtl) -> Duration {
    ttl.authorization_code.max(MIN_TTL)
}

pub(crate) fn device_ttl(ttl: LoginTtl, authorization: &DeviceAuthorization) -> Duration {
    let configured = ttl.device_code;
    let Some(expires_at_ms) = authorization.expires_at_ms else {
        return configured.max(MIN_TTL);
    };
    let remaining = expires_at_ms.saturating_sub(crate::rt::now_ms()).max(0);
    configured
        .min(Duration::from_millis(remaining as u64))
        .max(MIN_TTL)
}

pub(crate) async fn store(
    cache: &Arc<dyn Cache>,
    session_id: &str,
    session: &LoginSession,
    ttl: Duration,
) -> SdkResult<()> {
    let value = serde_json::to_vec(session)
        .map_err(|error| SdkError::invalid(format!("login session is not encodable: {error}")))?;
    cache.put(&key(session_id), value, ttl).await?;
    Ok(())
}

/// The session behind `session_id`, or [`SdkError::LoginExpired`]. A payload
/// that will not decode is treated as gone rather than as an internal error:
/// it can only come from a peer running another build, and the caller's remedy
/// is the same either way — start the login again.
pub(crate) async fn load(cache: &Arc<dyn Cache>, session_id: &str) -> SdkResult<LoginSession> {
    let entry = cache
        .get(&key(session_id))
        .await?
        .ok_or(SdkError::LoginExpired)?;
    serde_json::from_slice(&entry.value).map_err(|error| {
        tracing::warn!(%error, "a login session payload could not be decoded");
        SdkError::LoginExpired
    })
}

pub(crate) async fn delete(cache: &Arc<dyn Cache>, session_id: &str) -> SdkResult<()> {
    cache.delete(&key(session_id)).await?;
    Ok(())
}
