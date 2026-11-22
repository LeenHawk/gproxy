//! Which upstream session a caller's session uses on one server.
//!
//! The record is durable and written with compare-and-exchange, so any
//! instance can continue a session another opened and two instances opening
//! at once settle on one. It names the credential the session was opened
//! with: a session keeps it.

use std::time::{Duration, SystemTime};

use gproxy_protocol::{
    capability::{CapabilityError, CasResult, StateWrite, Version},
    connection::Bytes,
};
use gproxy_upstream::UpstreamState;
use serde::{Deserialize, Serialize};

use crate::client::Mode;

/// A binding nobody touched for this long is forgotten; the next request
/// opens a new one.
const LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub credential_id: String,
    pub mode: Mode,
}

/// A stored binding and the receipt that removes exactly it.
pub struct Stored {
    pub binding: Binding,
    version: Version,
}

/// The binding of `session` in `state`, which the host scopes to one server.
pub async fn load(
    state: &dyn UpstreamState,
    session: &str,
) -> Result<Option<Stored>, CapabilityError> {
    Ok(state.get(session).await?.and_then(|entry| {
        Some(Stored {
            binding: serde_json::from_slice(&entry.payload).ok()?,
            version: entry.version,
        })
    }))
}

/// Record a binding for a session that has none. `None` means another
/// request recorded one first; the caller reads it and gives up its own.
pub async fn create(
    state: &dyn UpstreamState,
    session: &str,
    binding: Binding,
) -> Result<Option<Stored>, CapabilityError> {
    let write = StateWrite {
        payload: Bytes::from(serde_json::to_vec(&binding).expect("a binding serializes")),
        expires_at: Some(SystemTime::now() + LIFETIME),
    };
    Ok(
        match state.compare_exchange(session, None, Some(write)).await? {
            CasResult::Applied(Some(version)) => Some(Stored { binding, version }),
            CasResult::Applied(None) | CasResult::Conflict => None,
        },
    )
}

/// Forget a binding, unless it has been replaced since it was read.
pub async fn remove(
    state: &dyn UpstreamState,
    session: &str,
    stored: Stored,
) -> Result<(), CapabilityError> {
    state
        .compare_exchange(session, Some(stored.version), None)
        .await
        .map(drop)
}
