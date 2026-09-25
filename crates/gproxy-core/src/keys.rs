//! Cache key grammar. The cache stores opaque bytes under opaque strings; this
//! module is the only place that composes those strings, so every reader and
//! writer agrees. Values are JSON of the `runtime` payload types.

use crate::CredentialAffinityKey;

const PREFIX: &str = "gproxy-core:v1";

/// Notification topic for `Invalidation` payloads.
pub const INVALIDATION_TOPIC: &str = "gproxy-core:v1:invalidation";

pub fn credential_blocks(provider_id: &str, credential_id: &str) -> String {
    format!("{PREFIX}:blocks:{provider_id}:{credential_id}")
}

pub fn credential_affinity(key: &CredentialAffinityKey) -> String {
    format!(
        "{PREFIX}:affinity:{}:{}:{}:{}",
        key.provider_id, key.scope.scope, key.scope.source as u8, key.scope.session_id
    )
}

pub fn credential_selection(provider_id: &str, candidate_signature: &str) -> String {
    format!("{PREFIX}:selection:{provider_id}:{candidate_signature}")
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
