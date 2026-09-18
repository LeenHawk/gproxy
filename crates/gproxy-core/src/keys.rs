//! Cache key grammar. The cache stores opaque bytes under opaque strings; this
//! module is the only place that composes those strings, so every reader and
//! writer agrees. Values are JSON of the `runtime` payload types.

use crate::{CountedWindowKey, CredentialAffinityKey};

const PREFIX: &str = "gproxy-core:v1";

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

pub fn counted_window(key: &CountedWindowKey) -> String {
    format!(
        "{PREFIX}:counted:{}:{}:{}",
        key.credential_id, key.dimension, key.window_start_ms
    )
}
