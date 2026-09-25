//! Channel-owned Claude refusal fallback policy. Core executes the plan through
//! the already selected credential; upstream model names remain channel-owned.

use super::{ConfigKey, ConfigKeyKind};

#[derive(Debug, Clone, Copy)]
pub struct ClaudeFallback {
    /// Whether a refusal credit can be redeemed by this upstream.
    pub credit: bool,
    /// Used by the provider's "default" mode.
    pub recommended_model: &'static str,
}

pub const CLAUDE_FALLBACK_KEYS: [ConfigKey; 2] = [
    ConfigKey::optional(
        "fallback_mode",
        ConfigKeyKind::String,
        "Claude refusal fallback: off (default), default (recommended model), or models.",
    ),
    ConfigKey::optional(
        "fallback_models",
        ConfigKeyKind::Json,
        "Ordered fallback model IDs; duplicates and the primary model are skipped, at most three.",
    ),
];

/// Keep a reseller namespace on bare Claude IDs, but leave explicit vendor IDs
/// and cloud-native IDs untouched.
pub fn claude_fallback_model(primary: &str, fallback: &str) -> String {
    if fallback.starts_with("claude-") {
        let prefix = primary.rfind("claude-").map_or("", |at| &primary[..at]);
        format!("{prefix}{fallback}")
    } else {
        fallback.to_owned()
    }
}
