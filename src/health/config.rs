//! Circuit-breaker thresholds (§3.2), parsed from
//! `providers.settings_json.circuit_breaker`, with optional per-route overrides
//! from `routes.settings_json.circuit_breaker` (see [`breaker_config_merged`]).

use serde::Deserialize;
use serde_json::Value;

/// Hard cap on the exponentially-backed-off cooldown.
pub const COOLDOWN_CAP_SECS: u64 = 300;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BreakerConfig {
    /// Open after this many consecutive failures.
    pub consecutive_failures: u32,
    /// Optional windowed error-rate trigger (off by default).
    pub error_rate: Option<ErrorRateCfg>,
    /// Base cooldown; doubles per consecutive open, capped at
    /// [`COOLDOWN_CAP_SECS`].
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorRateCfg {
    pub window_secs: u64,
    pub threshold: f64,
    pub min_requests: u32,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            consecutive_failures: 5,
            error_rate: None,
            cooldown_secs: 30,
        }
    }
}

/// Parse from provider settings; absent → default; malformed → default + warn.
pub fn breaker_config(provider_settings: &Value) -> BreakerConfig {
    match provider_settings.get("circuit_breaker") {
        None | Some(Value::Null) => BreakerConfig::default(),
        Some(v) => parse_breaker(v),
    }
}

/// Effective breaker config for a route member: the route's `circuit_breaker`
/// fields override the provider's, each missing field falling back to the
/// provider (then to defaults). Used only for the route-scoped MEMBER breaker;
/// the credential breaker stays provider-level since credentials are shared
/// across routes. `None` route settings → plain [`breaker_config`].
pub fn breaker_config_merged(
    route_settings: Option<&Value>,
    provider_settings: &Value,
) -> BreakerConfig {
    let route_cb = route_settings.and_then(|s| s.get("circuit_breaker"));
    let provider_cb = provider_settings.get("circuit_breaker");
    match (route_cb, provider_cb) {
        (None, _) => breaker_config(provider_settings),
        (Some(r), None) | (Some(r), Some(Value::Null)) => parse_breaker(r),
        (Some(r), Some(p)) => match (p.as_object(), r.as_object()) {
            // field-level overlay: provider base, route keys win
            (Some(pm), Some(rm)) => {
                let mut merged = pm.clone();
                merged.extend(rm.iter().map(|(k, v)| (k.clone(), v.clone())));
                parse_breaker(&Value::Object(merged))
            }
            // a non-object route override wins wholesale
            _ => parse_breaker(r),
        },
    }
}

/// Deserialize a `circuit_breaker` value; malformed → default + warn.
fn parse_breaker(v: &Value) -> BreakerConfig {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "malformed circuit_breaker settings; using defaults");
        BreakerConfig::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn route_override_wins_field_level_with_provider_fallback() {
        let provider =
            json!({ "circuit_breaker": { "consecutive_failures": 9, "cooldown_secs": 99 } });
        let route = json!({ "circuit_breaker": { "consecutive_failures": 2 } });

        let merged = breaker_config_merged(Some(&route), &provider);
        // route field wins …
        assert_eq!(merged.consecutive_failures, 2);
        // … and the omitted field falls back to the provider's value.
        assert_eq!(merged.cooldown_secs, 99);

        // No route settings → plain provider config.
        let provider_only = breaker_config_merged(None, &provider);
        assert_eq!(provider_only.consecutive_failures, 9);
        assert_eq!(provider_only.cooldown_secs, 99);
    }
}
