//! Per-instance passive health (§3.2/§16.3): member breakers, credential
//! and credential-model health (breaker + cooldown), member latency EWMA. Soft
//! state — restart clears, multi-instance deployments observe independently.

pub mod breaker;
pub mod config;
pub mod persist;

use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;

use breaker::{Admit, Breaker, Transition};
use config::BreakerConfig;

const EWMA_ALPHA: f64 = 0.3;

struct CredHealth {
    breaker: Breaker,
    cooldown_until: i64,
}

impl CredHealth {
    fn new() -> Self {
        Self {
            breaker: Breaker::new(),
            cooldown_until: 0,
        }
    }

    fn availability(&self, now: i64) -> CredAdmit {
        if self.cooldown_until > now {
            return CredAdmit::No;
        }
        map_admit(self.breaker.availability(now))
    }

    fn admit(&mut self, cfg: &BreakerConfig, now: i64) -> CredAdmit {
        if self.cooldown_until > now {
            return CredAdmit::No;
        }
        map_admit(self.breaker.admit(cfg, now))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CredentialModelKey {
    credential_id: i64,
    upstream_model_id: String,
}

impl CredentialModelKey {
    fn new(credential_id: i64, upstream_model_id: &str) -> Self {
        Self {
            credential_id,
            upstream_model_id: upstream_model_id.to_owned(),
        }
    }
}

/// Admission verdict for a credential (cooldown folded in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredAdmit {
    Yes,
    Probe,
    No,
}

/// Entries are created lazily on first touch.
#[derive(Default)]
pub struct HealthState {
    members: DashMap<i64, Breaker>,
    /// Credential-wide state, written only by operations without a model
    /// target (refresh, model-list, usage) or explicit global evidence.
    creds: DashMap<i64, CredHealth>,
    /// Model request state. There is deliberately no inference or promotion
    /// from these exact pairs to credential-wide state.
    credential_models: DashMap<CredentialModelKey, CredHealth>,
    /// member_id → latency EWMA (ms).
    latency_ms: DashMap<i64, f64>,
    /// route_id → rotation counter (round_robin / weighted member selection).
    /// Separate from `cred_rotation` — route ids and provider ids share the
    /// i64 space and must not collide.
    route_rotation: DashMap<i64, AtomicUsize>,
    /// provider_id → rotation counter (credential pool selection).
    cred_rotation: DashMap<i64, AtomicUsize>,
}

impl HealthState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn admit_member(&self, id: i64, cfg: &BreakerConfig, now: i64) -> Admit {
        self.members.entry(id).or_default().admit(cfg, now)
    }

    /// Rate-limit/auth cooldown takes precedence; otherwise the breaker rules.
    pub fn admit_credential(&self, id: i64, cfg: &BreakerConfig, now: i64) -> CredAdmit {
        self.creds
            .entry(id)
            .or_insert_with(CredHealth::new)
            .admit(cfg, now)
    }

    /// Side-effect-free credential admission check for candidate planning.
    pub fn credential_available(&self, id: i64, now: i64) -> CredAdmit {
        self.creds
            .get(&id)
            .map_or(CredAdmit::Yes, |health| health.availability(now))
    }

    /// Side-effect-free check of the credential-wide gate and this exact model.
    pub fn credential_model_available(
        &self,
        credential_id: i64,
        upstream_model_id: &str,
        now: i64,
    ) -> CredAdmit {
        let global = self.credential_available(credential_id, now);
        if global == CredAdmit::No {
            return CredAdmit::No;
        }
        let key = CredentialModelKey::new(credential_id, upstream_model_id);
        combine_admit(
            global,
            self.credential_models
                .get(&key)
                .map_or(CredAdmit::Yes, |health| health.availability(now)),
        )
    }

    /// Admit an actual model attempt. The credential-wide gate is checked but
    /// its half-open probe is not consumed: model outcomes never mutate or
    /// infer credential-wide health.
    pub fn admit_credential_model(
        &self,
        credential_id: i64,
        upstream_model_id: &str,
        cfg: &BreakerConfig,
        now: i64,
    ) -> CredAdmit {
        let global = self.credential_available(credential_id, now);
        if global == CredAdmit::No {
            return CredAdmit::No;
        }
        let key = CredentialModelKey::new(credential_id, upstream_model_id);
        let local = self
            .credential_models
            .get_mut(&key)
            .map_or(CredAdmit::Yes, |mut health| health.admit(cfg, now));
        combine_admit(global, local)
    }

    pub fn record_member(
        &self,
        id: i64,
        cfg: &BreakerConfig,
        ok: bool,
        now: i64,
    ) -> Option<Transition> {
        let mut b = self.members.entry(id).or_default();
        if ok {
            b.on_success(now)
        } else {
            b.on_failure(cfg, now)
        }
    }

    pub fn record_credential(
        &self,
        id: i64,
        cfg: &BreakerConfig,
        ok: bool,
        now: i64,
    ) -> Option<Transition> {
        let mut e = self.creds.entry(id).or_insert_with(CredHealth::new);
        if ok {
            e.breaker.on_success(now)
        } else {
            e.breaker.on_failure(cfg, now)
        }
    }

    pub fn record_credential_model(
        &self,
        credential_id: i64,
        upstream_model_id: &str,
        cfg: &BreakerConfig,
        ok: bool,
        now: i64,
    ) -> Option<Transition> {
        let key = CredentialModelKey::new(credential_id, upstream_model_id);
        let mut health = match self.credential_models.entry(key) {
            Entry::Occupied(entry) => entry.into_ref(),
            Entry::Vacant(_) if ok && cfg.error_rate.is_none() => return None,
            Entry::Vacant(entry) => entry.insert(CredHealth::new()),
        };
        if ok {
            health.breaker.on_success(now)
        } else {
            health.breaker.on_failure(cfg, now)
        }
    }

    /// 429/auth-dead cooldowns; keeps the later of two overlapping deadlines.
    pub fn cool_credential(&self, id: i64, until: i64) {
        let mut e = self.creds.entry(id).or_insert_with(CredHealth::new);
        e.cooldown_until = e.cooldown_until.max(until);
    }

    pub fn cool_credential_model(&self, credential_id: i64, upstream_model_id: &str, until: i64) {
        let key = CredentialModelKey::new(credential_id, upstream_model_id);
        let mut health = self
            .credential_models
            .entry(key)
            .or_insert_with(CredHealth::new);
        health.cooldown_until = health.cooldown_until.max(until);
    }

    /// EWMA with alpha 0.3; first sample is taken as-is.
    pub fn record_latency(&self, member_id: i64, ms: f64) {
        self.latency_ms
            .entry(member_id)
            .and_modify(|v| *v = *v * (1.0 - EWMA_ALPHA) + ms * EWMA_ALPHA)
            .or_insert(ms);
    }

    pub fn latency(&self, member_id: i64) -> Option<f64> {
        self.latency_ms.get(&member_id).map(|v| *v)
    }

    /// Monotonic per-route counter for member rotation.
    pub fn next_route_rotation(&self, route_id: i64) -> usize {
        self.route_rotation
            .entry(route_id)
            .or_default()
            .fetch_add(1, Ordering::Relaxed)
    }

    /// Monotonic per-provider counter for credential rotation.
    pub fn next_credential_rotation(&self, provider_id: i64) -> usize {
        self.cred_rotation
            .entry(provider_id)
            .or_default()
            .fetch_add(1, Ordering::Relaxed)
    }
}

fn map_admit(admit: Admit) -> CredAdmit {
    match admit {
        Admit::Yes => CredAdmit::Yes,
        Admit::Probe => CredAdmit::Probe,
        Admit::No { .. } => CredAdmit::No,
    }
}

fn combine_admit(global: CredAdmit, local: CredAdmit) -> CredAdmit {
    match (global, local) {
        (CredAdmit::No, _) | (_, CredAdmit::No) => CredAdmit::No,
        (CredAdmit::Probe, _) | (_, CredAdmit::Probe) => CredAdmit::Probe,
        _ => CredAdmit::Yes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cred_cooldown_blocks_until_expiry_and_ewma_tracks_samples() {
        let h = HealthState::new();
        let cfg = BreakerConfig::default();

        h.cool_credential(1, 200);
        assert_eq!(h.admit_credential(1, &cfg, 150), CredAdmit::No);
        assert_eq!(h.admit_credential(1, &cfg, 200), CredAdmit::Yes);

        h.record_latency(7, 100.0);
        assert_eq!(h.latency(7), Some(100.0));
        h.record_latency(7, 200.0);
        assert!((h.latency(7).unwrap() - 130.0).abs() < 1e-9);
        assert_eq!(h.latency(8), None);
    }

    #[test]
    fn model_health_is_exact_and_global_health_dominates() {
        let h = HealthState::new();
        let cfg = BreakerConfig {
            consecutive_failures: 1,
            ..BreakerConfig::default()
        };

        h.record_credential_model(1, "model-a", &cfg, false, 100);
        assert_eq!(
            h.credential_model_available(1, "model-a", 100),
            CredAdmit::No
        );
        assert_eq!(
            h.credential_model_available(1, "model-b", 100),
            CredAdmit::Yes
        );
        assert_eq!(h.credential_available(1, 100), CredAdmit::Yes);

        h.record_credential_model(1, "model-b", &cfg, false, 100);
        assert_eq!(h.credential_available(1, 100), CredAdmit::Yes);

        h.cool_credential(1, 200);
        assert_eq!(
            h.credential_model_available(1, "model-b", 150),
            CredAdmit::No
        );
    }
}
