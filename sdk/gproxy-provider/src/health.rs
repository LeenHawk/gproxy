use std::collections::HashMap;
use std::time::Instant;

/// Per-credential health tracking.
///
/// Each channel defines its own health shape via this trait's associated type
/// on [`Channel`](crate::Channel). Channels with per-model rate limits use
/// a map of cooldowns; simpler channels can use a boolean.
pub trait CredentialHealth: Send + Sync + Clone + Default + 'static {
    /// Whether this credential is available for a request targeting `model`.
    /// Pass `None` if the request doesn't specify a model.
    fn is_available(&self, model: Option<&str>) -> bool;

    /// Human-readable health category for operator surfaces.
    fn status(&self, model: Option<&str>) -> &'static str {
        if self.is_available(model) {
            "healthy"
        } else {
            "unavailable"
        }
    }

    /// Record a failed upstream response.
    fn record_error(&mut self, status: u16, model: Option<&str>, retry_after_ms: Option<u64>);

    /// Record a successful upstream response.
    fn record_success(&mut self, model: Option<&str>);
}

/// Simple health implementation: globally healthy or dead.
#[derive(Debug, Clone, Default)]
pub struct SimpleHealth {
    pub dead: bool,
}

impl CredentialHealth for SimpleHealth {
    fn is_available(&self, _model: Option<&str>) -> bool {
        !self.dead
    }

    fn status(&self, _model: Option<&str>) -> &'static str {
        if self.dead {
            "unavailable"
        } else {
            "healthy"
        }
    }

    fn record_error(&mut self, status: u16, _model: Option<&str>, _retry_after_ms: Option<u64>) {
        if status == 401 || status == 403 {
            self.dead = true;
        }
    }

    fn record_success(&mut self, _model: Option<&str>) {
        self.dead = false;
    }
}

/// Per-model cooldown health: tracks rate limits per model name.
///
/// When a 429 arrives with a `retry-after` header the cooldown duration comes
/// directly from the server.  When no `retry-after` is present the health
/// falls back to capped exponential back-off: 1 s → 2 s → … → 60 s, reset
/// on the next success.
#[derive(Debug, Clone, Default)]
pub struct ModelCooldownHealth {
    pub dead: bool,
    pub model_cooldowns: HashMap<String, Instant>,
    pub global_cooldown: Option<Instant>,
    /// Number of consecutive back-off errors (no explicit retry-after).
    /// Reset to 0 on success.  Used to compute exponential back-off:
    /// `min(2^backoff_count * 1000, 60_000)` ms.
    pub backoff_count: u32,
}

impl CredentialHealth for ModelCooldownHealth {
    fn is_available(&self, model: Option<&str>) -> bool {
        if self.dead {
            return false;
        }
        let now = Instant::now();
        if self.global_cooldown.is_some_and(|until| now < until) {
            return false;
        }
        if let Some(model) = model
            && let Some(until) = self.model_cooldowns.get(model)
            && now < *until
        {
            return false;
        }
        true
    }

    fn status(&self, model: Option<&str>) -> &'static str {
        if self.dead {
            return "unavailable";
        }
        let now = Instant::now();
        if self.global_cooldown.is_some_and(|until| now < until) {
            return "cooldown";
        }
        if let Some(model) = model {
            if self
                .model_cooldowns
                .get(model)
                .is_some_and(|until| now < *until)
            {
                return "cooldown";
            }
        } else if self.model_cooldowns.values().any(|until| now < *until) {
            return "cooldown";
        }
        "healthy"
    }

    fn record_error(&mut self, status: u16, model: Option<&str>, retry_after_ms: Option<u64>) {
        if status == 401 || status == 403 {
            self.dead = true;
            return;
        }
        let cooldown = match retry_after_ms {
            Some(ms) => ms,
            None => {
                let ms = 1000u64
                    .saturating_mul(1u64 << self.backoff_count.min(20))
                    .min(60_000);
                self.backoff_count = self.backoff_count.saturating_add(1);
                ms
            }
        };
        let until = Instant::now() + std::time::Duration::from_millis(cooldown);
        if let Some(model) = model {
            self.model_cooldowns.insert(model.to_string(), until);
        } else {
            self.global_cooldown = Some(until);
        }
    }

    fn record_success(&mut self, _model: Option<&str>) {
        self.dead = false;
        self.backoff_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{CredentialHealth, ModelCooldownHealth};

    #[test]
    fn model_cooldown_health_status_reports_cooldown() {
      let mut health = ModelCooldownHealth::default();
      health.global_cooldown = Some(Instant::now() + Duration::from_secs(30));
      assert_eq!(health.status(None), "cooldown");
    }

    #[test]
    fn model_cooldown_health_status_reports_unavailable_when_dead() {
      let health = ModelCooldownHealth {
        dead: true,
        ..Default::default()
      };
      assert_eq!(health.status(None), "unavailable");
    }
}
