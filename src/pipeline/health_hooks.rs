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
use crate::health::persist::persist_credential_transition;
use crate::pipeline::context::Candidate;
use crate::util::time::unix_now;

/// Cooldown for a 429 without `Retry-After`.
const RATE_LIMIT_DEFAULT: Duration = Duration::from_secs(30);
/// Cooldown for an auth-dead credential (refresh lands M7).
const AUTH_DEAD_SECS: i64 = 600;

/// Record one attempt's outcome. `send_ms` is the measured send latency
/// (native only; `None` on wasm and on failures).
pub fn record_attempt(
    state: &AppState,
    cand: &Candidate,
    disposition: &Disposition,
    send_ms: Option<f64>,
) {
    let now = unix_now();
    // The member breaker honours the route override (carried on the candidate);
    // the credential breaker is provider-scoped (credentials are shared across
    // routes), so it uses the plain provider config.
    let member_cfg = &cand.breaker_cfg;
    let cred_cfg = breaker_config(&cand.provider.settings_json);
    let cred_id = cand.credential.id;
    match disposition {
        Disposition::Success => {
            if let Some(mid) = cand.member_id {
                state.health.record_member(mid, member_cfg, true, now);
                if let Some(ms) = send_ms {
                    state.health.record_latency(mid, ms);
                }
            }
            let t = state
                .health
                .record_credential(cred_id, &cred_cfg, true, now);
            persist_breaker_edge(state, cand, t);
        }
        Disposition::Transient => {
            if let Some(mid) = cand.member_id {
                state.health.record_member(mid, member_cfg, false, now);
            }
            let t = state
                .health
                .record_credential(cred_id, &cred_cfg, false, now);
            persist_breaker_edge(state, cand, t);
        }
        // A rate-limited key says nothing about the member — member untouched.
        Disposition::RateLimited { retry_after } => {
            let until = now + retry_after.unwrap_or(RATE_LIMIT_DEFAULT).as_secs() as i64;
            state.health.cool_credential(cred_id, until);
            persist_cooldown(state, cand, "rate_limited", until, "429 rate limited");
        }
        Disposition::AuthDead => {
            let until = now + AUTH_DEAD_SECS;
            state.health.cool_credential(cred_id, until);
            persist_cooldown(state, cand, "auth_dead", until, "auth dead (401/402/403)");
        }
        // Client error returned to the caller — no health impact.
        Disposition::Permanent => {}
    }
}

/// Transport (`send_once` Err) and prepare failures count as `Transient`.
pub fn record_failure(state: &AppState, cand: &Candidate) {
    record_attempt(state, cand, &Disposition::Transient, None);
}

/// §16.3: persist a credential breaker transition edge (Opened/Reopened →
/// "breaker", Closed → "recovered"). No-op when no transition occurred.
fn persist_breaker_edge(state: &AppState, cand: &Candidate, t: Option<Transition>) {
    let Some(t) = t else { return };
    let (kind, json, last_error) = match t {
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
    };
    persist_credential_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        cand.credential.id,
        cand.provider.channel.clone(),
        kind,
        json,
        last_error,
    );
}

/// §16.3: persist a rate-limited / auth-dead cooldown edge.
fn persist_cooldown(state: &AppState, cand: &Candidate, kind: &'static str, until: i64, why: &str) {
    persist_credential_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        cand.credential.id,
        cand.provider.channel.clone(),
        kind,
        json!({ "state": "cooldown", "open_until": until, "reason": why }),
        Some(why.to_string()),
    );
}
