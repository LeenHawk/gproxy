//! Per-attempt disposition → health recording (§3.2/§6.4) plus §16.3
//! edge-triggered persistence of credential transitions. Member-breaker
//! transitions stay memory-only — `credential_statuses` is credential-scoped.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use crate::app::AppState;
use crate::channel::Disposition;
use crate::health::breaker::Transition;
use crate::health::config::breaker_config;
use crate::health::persist::CredentialModelTransition;
use crate::health::persist::persist_credential_transition;
use crate::pipeline::context::Candidate;
use crate::store::persistence::records::{Credential, Provider};
use crate::util::time::unix_now;

/// Cooldown for a 429 without `Retry-After`.
const RATE_LIMIT_DEFAULT: Duration = Duration::from_secs(30);
/// Cooldown for an auth-dead credential (refresh lands M7).
const AUTH_DEAD_SECS: i64 = 600;

/// Record one model-bound attempt. Credential health is keyed by the exact
/// final upstream model; no result here is promoted to credential-wide health.
/// `send_ms` is the measured send latency (native only; `None` on failures).
pub fn record_attempt(
    state: &AppState,
    cand: &Candidate,
    disposition: &Disposition,
    send_ms: Option<f64>,
) {
    let now = unix_now();
    let cred_cfg = breaker_config(&cand.provider.settings_json);
    let cred_id = cand.credential.id;
    let model = &cand.upstream_model_id;
    match disposition {
        Disposition::Success => {
            if let Some(mid) = cand.member_id
                && let Some(ms) = send_ms
            {
                state.health.record_latency(mid, ms);
            }
            let t = state
                .health
                .record_credential_model(cred_id, model, &cred_cfg, true, now);
            persist_model_breaker_edge(state, cand, t);
        }
        Disposition::Transient => {
            let t = state
                .health
                .record_credential_model(cred_id, model, &cred_cfg, false, now);
            persist_model_breaker_edge(state, cand, t);
        }
        Disposition::RateLimited { retry_after } => {
            let until = now + retry_after.unwrap_or(RATE_LIMIT_DEFAULT).as_secs() as i64;
            state.health.cool_credential_model(cred_id, model, until);
            persist_model_cooldown(state, cand, "rate_limited", until, "429 rate limited");
        }
        Disposition::AuthDead => {
            let until = now + AUTH_DEAD_SECS;
            state.health.cool_credential_model(cred_id, model, until);
            persist_model_cooldown(
                state,
                cand,
                "auth_dead",
                until,
                "model authentication rejected (401/402/403)",
            );
        }
        // Client error returned to the caller — no health impact.
        Disposition::Permanent => {}
    }
}

/// Record an operation that targets the credential itself rather than a model:
/// refresh, model-list, usage, account/quota and similar endpoints.
pub fn record_credential_attempt(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    disposition: &Disposition,
) {
    let now = unix_now();
    let cfg = breaker_config(&provider.settings_json);
    let transition = match disposition {
        Disposition::Success => state
            .health
            .record_credential(credential.id, &cfg, true, now),
        Disposition::Transient => state
            .health
            .record_credential(credential.id, &cfg, false, now),
        Disposition::RateLimited { retry_after } => {
            let until = now + retry_after.unwrap_or(RATE_LIMIT_DEFAULT).as_secs() as i64;
            state.health.cool_credential(credential.id, until);
            persist_credential_cooldown(
                state,
                provider,
                credential,
                "rate_limited",
                until,
                "429 rate limited",
            );
            return;
        }
        Disposition::AuthDead => {
            let until = now + AUTH_DEAD_SECS;
            state.health.cool_credential(credential.id, until);
            persist_credential_cooldown(
                state,
                provider,
                credential,
                "auth_dead",
                until,
                "credential authentication rejected",
            );
            return;
        }
        Disposition::Permanent => return,
    };
    persist_credential_breaker_edge(state, provider, credential, transition);
}

/// Transport (`send_once` Err) and prepare failures count as `Transient`.
pub fn record_failure(state: &AppState, cand: &Candidate) {
    record_attempt(state, cand, &Disposition::Transient, None);
}

/// §16.3: persist a credential breaker transition edge (Opened/Reopened →
/// "breaker", Closed → "recovered"). No-op when no transition occurred.
fn breaker_transition(
    t: Option<Transition>,
) -> Option<(&'static str, serde_json::Value, Option<String>)> {
    let t = t?;
    Some(match t {
        Transition::Opened {
            until,
            consecutive_failures,
        } => (
            "breaker",
            json!({
                "state": "open",
                "open_until": until,
                "consecutive_failures": consecutive_failures,
                "reason": "breaker opened",
            }),
            Some("breaker opened".to_string()),
        ),
        Transition::Reopened { until } => (
            "breaker",
            json!({
                "state": "open",
                "open_until": until,
                "reason": "probe failed; breaker reopened",
            }),
            Some("probe failed; breaker reopened".to_string()),
        ),
        Transition::Closed => (
            "recovered",
            json!({ "state": "closed", "reason": "probe succeeded; breaker closed" }),
            None,
        ),
    })
}

fn persist_credential_breaker_edge(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    transition: Option<Transition>,
) {
    let Some((kind, json, last_error)) = breaker_transition(transition) else {
        return;
    };
    persist_credential_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        credential.id,
        provider.channel.clone(),
        kind,
        json,
        last_error,
    );
}

fn persist_credential_cooldown(
    state: &AppState,
    provider: &Provider,
    credential: &Credential,
    kind: &'static str,
    until: i64,
    why: &str,
) {
    persist_credential_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        credential.id,
        provider.channel.clone(),
        kind,
        json!({ "state": "cooldown", "open_until": until, "reason": why }),
        Some(why.to_string()),
    );
}

fn persist_model_breaker_edge(state: &AppState, cand: &Candidate, transition: Option<Transition>) {
    let Some((kind, json, last_error)) = breaker_transition(transition) else {
        return;
    };
    crate::health::persist::persist_credential_model_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        CredentialModelTransition {
            credential_id: cand.credential.id,
            channel: cand.provider.channel.clone(),
            model_id: cand.upstream_model_id.clone(),
            kind,
            json,
            last_error,
        },
    );
}

fn persist_model_cooldown(
    state: &AppState,
    cand: &Candidate,
    kind: &'static str,
    until: i64,
    why: &str,
) {
    crate::health::persist::persist_credential_model_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        CredentialModelTransition {
            credential_id: cand.credential.id,
            channel: cand.provider.channel.clone(),
            model_id: cand.upstream_model_id.clone(),
            kind,
            json: json!({ "state": "cooldown", "open_until": until, "reason": why }),
            last_error: Some(why.to_string()),
        },
    );
}
