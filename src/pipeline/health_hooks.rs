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
use crate::health::persist::{CredentialModelTransition, CredentialTransition};
use crate::pipeline::context::Candidate;
use crate::store::persistence::records::{Credential, Provider};
use crate::util::time::unix_now;

/// Cooldown for a 429 without `Retry-After`.
const RATE_LIMIT_DEFAULT: Duration = Duration::from_secs(30);
/// Cooldown for an auth-dead credential (refresh lands M7).
const AUTH_DEAD_SECS: i64 = 600;

/// Record one model-bound attempt. Credential health is keyed by the exact
/// final upstream model, with two channel-declared promotions:
/// - 429 on a model drawing from the account-wide MAIN quota pool
///   ([`shares_account_quota`](crate::channel::Channel::shares_account_quota))
///   cools the whole credential — the main limit governs every model, so
///   separate-limit models (codex spark, claude fable) are blocked too. Their
///   OWN 429s stay model-scoped: only the additional scoped limit is hit, the
///   main pool may still have budget;
/// - auth-dead on account-wide-auth channels
///   ([`credential_wide_auth`](crate::channel::Channel::credential_wide_auth))
///   cools the whole credential (the token is dead for every model).
///
/// Breaker outcomes stay model-scoped. `send_ms` is the measured send latency
/// (native only; `None` on failures).
pub fn record_attempt(
    state: &AppState,
    request_id: &str,
    cand: &Candidate,
    disposition: &Disposition,
    send_ms: Option<f64>,
) {
    let now = unix_now();
    let cred_cfg = breaker_config(&cand.provider.settings_json);
    let cred_id = cand.credential.id;
    let model = &cand.upstream_model_id;
    let channel = state.channels.get(&cand.provider.channel);
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
            persist_model_breaker_edge(state, request_id, cand, t);
        }
        Disposition::Transient => {
            let t = state
                .health
                .record_credential_model(cred_id, model, &cred_cfg, false, now);
            persist_model_breaker_edge(state, request_id, cand, t);
        }
        Disposition::RateLimited { retry_after } => {
            let until = now + retry_after.unwrap_or(RATE_LIMIT_DEFAULT).as_secs() as i64;
            if channel.is_some_and(|ch| ch.shares_account_quota(model)) {
                state.health.cool_credential(cred_id, until);
                persist_credential_cooldown(
                    state,
                    Some(request_id),
                    &cand.provider,
                    &cand.credential,
                    "rate_limited",
                    until,
                    "429 rate limited (account-wide main quota)",
                );
            } else {
                state.health.cool_credential_model(cred_id, model, until);
                persist_model_cooldown(
                    state,
                    request_id,
                    cand,
                    "rate_limited",
                    until,
                    "429 rate limited",
                );
            }
        }
        Disposition::AuthDead => {
            let until = now + AUTH_DEAD_SECS;
            if channel.is_some_and(|ch| ch.credential_wide_auth()) {
                state.health.cool_credential(cred_id, until);
                persist_credential_cooldown(
                    state,
                    Some(request_id),
                    &cand.provider,
                    &cand.credential,
                    "auth_dead",
                    until,
                    "credential authentication rejected (401/402/403)",
                );
            } else {
                state.health.cool_credential_model(cred_id, model, until);
                persist_model_cooldown(
                    state,
                    request_id,
                    cand,
                    "auth_dead",
                    until,
                    "model authentication rejected (401/402/403)",
                );
            }
        }
        // Client error returned to the caller — no health impact.
        Disposition::Permanent => {}
    }
}

/// Record an operation that targets the credential itself rather than a model:
/// refresh, model-list, usage, account/quota and similar endpoints.
pub fn record_credential_attempt(
    state: &AppState,
    request_id: Option<&str>,
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
                request_id,
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
                request_id,
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
    persist_credential_breaker_edge(state, request_id, provider, credential, transition);
}

/// Transport (`send_once` Err) and prepare failures count as `Transient`.
pub fn record_failure(state: &AppState, request_id: &str, cand: &Candidate) {
    record_attempt(state, request_id, cand, &Disposition::Transient, None);
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
    request_id: Option<&str>,
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
        CredentialTransition {
            request_id: request_id.map(str::to_owned),
            credential_id: credential.id,
            channel: provider.channel.clone(),
            kind,
            json,
            last_error,
        },
    );
}

fn persist_credential_cooldown(
    state: &AppState,
    request_id: Option<&str>,
    provider: &Provider,
    credential: &Credential,
    kind: &'static str,
    until: i64,
    why: &str,
) {
    persist_credential_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        CredentialTransition {
            request_id: request_id.map(str::to_owned),
            credential_id: credential.id,
            channel: provider.channel.clone(),
            kind,
            json: json!({ "state": "cooldown", "open_until": until, "reason": why }),
            last_error: Some(why.to_string()),
        },
    );
}

fn persist_model_breaker_edge(
    state: &AppState,
    request_id: &str,
    cand: &Candidate,
    transition: Option<Transition>,
) {
    let Some((kind, json, last_error)) = breaker_transition(transition) else {
        return;
    };
    crate::health::persist::persist_credential_model_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        CredentialModelTransition {
            request_id: Some(request_id.to_owned()),
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
    request_id: &str,
    cand: &Candidate,
    kind: &'static str,
    until: i64,
    why: &str,
) {
    crate::health::persist::persist_credential_model_transition(
        Arc::clone(&state.persistence),
        state.config.instance_id,
        CredentialModelTransition {
            request_id: Some(request_id.to_owned()),
            credential_id: cand.credential.id,
            channel: cand.provider.channel.clone(),
            model_id: cand.upstream_model_id.clone(),
            kind,
            json: json!({ "state": "cooldown", "open_until": until, "reason": why }),
            last_error: Some(why.to_string()),
        },
    );
}
