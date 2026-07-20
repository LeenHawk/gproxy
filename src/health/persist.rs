//! Fire-and-forget §16.3 edge persistence: credential and credential-model
//! health transitions are written asynchronously; a write failure never affects
//! the request. Native-only; edge skips (no detached tasks on wasm).
//!
//! NOTE: upserts key global state on `(credential_id, channel)` and model state
//! on `(credential_id, channel, model_id)`. Both are latest-state-wins snapshots,
//! not append-only event logs. Each instance stamps its `instance_id` into
//! `health_json`.

use std::sync::Arc;

use crate::store::persistence::PersistenceBackend;

pub struct CredentialModelTransition {
    pub credential_id: i64,
    pub channel: String,
    pub model_id: String,
    pub kind: &'static str,
    pub json: serde_json::Value,
    pub last_error: Option<String>,
}

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

/// Persist one health transition scoped to the final upstream model id.
#[cfg(not(target_arch = "wasm32"))]
pub fn persist_credential_model_transition(
    persistence: Arc<dyn PersistenceBackend>,
    instance_id: u64,
    transition: CredentialModelTransition,
) {
    let CredentialModelTransition {
        credential_id,
        channel,
        model_id,
        kind,
        mut json,
        last_error,
    } = transition;
    if let Some(obj) = json.as_object_mut() {
        obj.insert("instance_id".into(), serde_json::json!(instance_id));
    }
    let input = crate::store::persistence::records::CredentialModelStatusInput {
        id: None,
        credential_id,
        channel,
        model_id,
        health_kind: kind.to_string(),
        health_json: Some(json),
        checked_at: Some(crate::util::time::unix_now()),
        last_error,
    };
    tokio::spawn(async move {
        if let Err(e) = persistence.upsert_credential_model_status(input).await {
            tracing::warn!(error = %e, credential_id, "credential model health persist failed");
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

/// Edge follows credential-health persistence policy and skips detached writes.
#[cfg(target_arch = "wasm32")]
pub fn persist_credential_model_transition(
    _persistence: Arc<dyn PersistenceBackend>,
    _instance_id: u64,
    _transition: CredentialModelTransition,
) {
}
