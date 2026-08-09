//! Failed failover-attempt audit persistence.

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use crate::app::AppState;
use crate::billing::{self, FailureRecord};
use crate::http::redaction::{body_string, warn_unless_redacted};
use crate::pipeline::context::Candidate;
use crate::util::time::unix_now;

/// One failed failover attempt's wire facts, for the audit row.
pub struct FailedAttempt<'a> {
    pub url: &'a str,
    pub method: &'a str,
    pub status: i64,
    pub latency_ms: i64,
    pub error: &'a str,
    /// Buffered provider response bytes, when the transport exposed them.
    pub response_body: Option<&'a [u8]>,
}

/// Audit one failed failover attempt (`upstream_requests`, never billed).
/// Gated by `enable_upstream_log` (§8-D/§8-E). Fire-and-forget on native;
/// persisted inline on wasm so the request context remains alive.
pub async fn audit_failure(
    state: &AppState,
    request_id: &str,
    cand: &Candidate,
    attempt: FailedAttempt<'_>,
) {
    if !state.cp().log_settings.enable_upstream_log {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    spawn_native(state, request_id, cand, attempt);
    #[cfg(target_arch = "wasm32")]
    persist_edge(state, request_id, cand, attempt).await;
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_native(state: &AppState, request_id: &str, cand: &Candidate, attempt: FailedAttempt<'_>) {
    let persistence = Arc::clone(&state.persistence);
    let (provider_id, credential_id) = (cand.provider.id, cand.credential.id);
    let upstream_model = cand.upstream_model_id.clone();
    let (status, latency_ms) = (attempt.status, attempt.latency_ms);
    let at = unix_now();
    let settings = state.cp().log_settings.clone();
    let response_body = attempt
        .response_body
        .filter(|_| settings.enable_upstream_log_body)
        .map(|body| body_string(body, warn_unless_redacted(&settings)));
    let (request_id, url, method, error) = (
        request_id.to_owned(),
        attempt.url.to_owned(),
        attempt.method.to_owned(),
        attempt.error.to_owned(),
    );
    tokio::spawn(async move {
        let rec = FailureRecord {
            request_id: &request_id,
            at,
            provider_id: Some(provider_id),
            credential_id: Some(credential_id),
            url: &url,
            method: &method,
            status,
            latency_ms,
            error: &error,
            response_body: response_body.as_deref(),
        };
        if let Err(e) = billing::record_failure(persistence.as_ref(), rec).await {
            tracing::warn!(
                request_id = %request_id,
                provider_id,
                credential_id,
                upstream_model = %upstream_model,
                error = %e,
                "failed-attempt audit write failed"
            );
        }
    });
}

#[cfg(target_arch = "wasm32")]
async fn persist_edge(
    state: &AppState,
    request_id: &str,
    cand: &Candidate,
    attempt: FailedAttempt<'_>,
) {
    let settings = state.cp().log_settings.clone();
    let response_body = attempt
        .response_body
        .filter(|_| settings.enable_upstream_log_body)
        .map(|body| body_string(body, warn_unless_redacted(&settings)));
    let rec = FailureRecord {
        request_id,
        at: unix_now(),
        provider_id: Some(cand.provider.id),
        credential_id: Some(cand.credential.id),
        url: attempt.url,
        method: attempt.method,
        status: attempt.status,
        latency_ms: attempt.latency_ms,
        error: attempt.error,
        response_body: response_body.as_deref(),
    };
    if let Err(e) = billing::record_failure(state.persistence.as_ref(), rec).await {
        tracing::warn!(
            request_id,
            provider_id = cand.provider.id,
            credential_id = cand.credential.id,
            upstream_model = %cand.upstream_model_id,
            error = %e,
            "failed-attempt audit write failed"
        );
    }
}
