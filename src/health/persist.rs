//! Fire-and-forget §16.3 edge persistence: credential health transitions are
//! written to `credential_statuses` asynchronously — a write failure never
//! affects the request. Native-only; edge skips (no detached tasks on wasm).
//!
//! NOTE: the upsert keys on `(credential_id, channel)`, so this is a
//! latest-state-wins snapshot per credential/channel pair — not an append-only
//! event log. History across instances comes from each instance overwriting
//! with its own `instance_id` stamped into `health_json`.

use std::sync::Arc;

use crate::store::persistence::PersistenceBackend;

/// Persist one credential health transition (§16.3). `kind` is the
/// `health_kind` ("breaker" | "recovered" | "rate_limited" | "auth_dead");
/// `json` becomes `health_json` with `instance_id` stamped in.
#[cfg(not(target_arch = "wasm32"))]
pub fn persist_credential_transition(
    persistence: Arc<dyn PersistenceBackend>,
    instance_id: u64,
    credential_id: i64,
    channel: String,
    kind: &'static str,
    mut json: serde_json::Value,
    last_error: Option<String>,
) {
    if let Some(obj) = json.as_object_mut() {
        obj.insert("instance_id".into(), serde_json::json!(instance_id));
    }
    let input = crate::store::persistence::records::CredentialStatusInput {
        id: None,
        credential_id,
        channel,
        health_kind: kind.to_string(),
        health_json: Some(json),
        checked_at: Some(crate::util::time::unix_now()),
        last_error,
    };
    tokio::spawn(async move {
        if let Err(e) = persistence.upsert_credential_status(input).await {
            tracing::warn!(error = %e, credential_id, "credential health persist failed");
        }
    });
}

/// Edge: §16.3 persistence is skipped — the wasm runtime has no detached
/// tasks, and health state is per-isolate soft state anyway.
#[cfg(target_arch = "wasm32")]
pub fn persist_credential_transition(
    _persistence: Arc<dyn PersistenceBackend>,
    _instance_id: u64,
    _credential_id: i64,
    _channel: String,
    _kind: &'static str,
    _json: serde_json::Value,
    _last_error: Option<String>,
) {
}
