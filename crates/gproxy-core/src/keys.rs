//! Cache key grammar. The cache stores opaque bytes under opaque strings; this
//! module is the only place that composes those strings, so every reader and
//! writer agrees. Values are JSON of the `runtime` payload types.

use sha2::{Digest, Sha256};

use crate::CredentialAffinityKey;

const PREFIX: &str = "gproxy-core:v1";

/// Notification topic for `Invalidation` payloads.
pub const INVALIDATION_TOPIC: &str = "gproxy-core:v1:invalidation";

pub fn credential_blocks(provider_id: &str, credential_id: &str) -> String {
    format!("{PREFIX}:blocks:{provider_id}:{credential_id}")
}

/// The session half is digested: a session id is whatever the client sent,
/// and spelled out it could push the key past the cache's key limit and turn
/// every read of it into an error.
pub fn credential_affinity(key: &CredentialAffinityKey) -> String {
    format!(
        "{PREFIX}:affinity:{}:{}",
        key.provider_id,
        digest(&[
            key.scope.scope.as_bytes(),
            &[key.scope.source as u8],
            key.scope.session_id.as_bytes(),
        ])
    )
}

/// The signature is every eligible credential id, so it grows with the pool;
/// digested, the key stays the same length at any pool size.
pub fn credential_selection(provider_id: &str, candidate_signature: &str) -> String {
    format!(
        "{PREFIX}:selection:{provider_id}:{}",
        digest(&[candidate_signature.as_bytes()])
    )
}

/// SHA-256 of `parts`, hex. Each part is length-prefixed so that no choice
/// of caller-controlled strings can make two different tuples collide.
fn digest(parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A Gemini resumable upload session, by the gateway token handed to the
/// client (`owned::upload`). The token is core's own random hex.
pub fn upload_session(token: &str) -> String {
    format!("{PREFIX}:upload:{token}")
}

pub fn refresh_lease(credential_id: &str) -> String {
    format!("{PREFIX}:refresh:{credential_id}")
}

/// Length-prefix isolation components to keep opaque caller scopes distinct.
pub fn realtime_call(provider: &str, scope: &str, call_id: &str) -> String {
    format!(
        "{PREFIX}:rtc:{}:{provider}:{}:{scope}:{call_id}",
        provider.len(),
        scope.len()
    )
}

pub fn realtime_response(
    provider: &str,
    scope: &str,
    credential: &str,
    call: Option<&str>,
    response: &str,
) -> String {
    let call = call.unwrap_or("");
    format!(
        "{PREFIX}:rtc-usage:{}:{provider}:{}:{scope}:{}:{credential}:{}:{call}:{response}",
        provider.len(),
        scope.len(),
        credential.len(),
        call.len()
    )
}

/// Short-lived projection of the latest persisted quota observations.
/// Observations update a warm projection; a cold cache is rebuilt from Store.
pub fn credential_reset_observations(credential_id: &str) -> String {
    format!("{PREFIX}:reset-observations:{credential_id}")
}

/// Latest persisted quota observation per entry id, compared against new
/// observations so unchanged readings skip the observation log.
pub fn credential_quota_observations(credential_id: &str) -> String {
    format!("{PREFIX}:quota-observations:{credential_id}")
}

/// The open `credential_cycles` of one credential, as the settlement path
/// reads them. Store arbitrates; this is a short-lived hint.
pub fn credential_cycles(credential_id: &str) -> String {
    format!("{PREFIX}:cycles:{credential_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_key_stays_short_at_any_pool_size() {
        let ids: Vec<String> = (0..1000).map(|i| format!("{i:032x}")).collect();
        let key = credential_selection("provider", &ids.join(","));
        assert!(key.len() < 128, "{}", key.len());
        assert_ne!(key, credential_selection("provider", &ids[1..].join(",")));
    }
}
