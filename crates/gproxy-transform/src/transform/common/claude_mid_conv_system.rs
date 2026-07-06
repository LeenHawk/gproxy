//! Claude model capability gate for mid-conversation system turns
//! (`mid_conv_system` content block / `system` message role).
//!
//! The Claude API surfaces both as a system-role turn internally; models
//! released before Opus 4.8 reject them with `role 'system' is not supported
//! on this model`. For those, a mid-conversation system message must be
//! downgraded to a plain assistant turn instead.

/// Models released before Opus 4.8 — a closed, enumerable set. Substring match
/// tolerates vendor prefixes (`anthropic.`, `us.anthropic.`) and date/version
/// suffixes (`claude-opus-4-20250514` hits `claude-opus-4-2`).
const PRE_OPUS_48: &[&str] = &[
    "claude-instant",
    "claude-1",
    "claude-2",
    "claude-3",
    "claude-sonnet-4",
    "claude-haiku-4",
    "claude-4-",
    "claude-opus-4-0",
    "claude-opus-4-1",
    "claude-opus-4-2",
    "claude-opus-4-3",
    "claude-opus-4-4",
    "claude-opus-4-5",
    "claude-opus-4-6",
    "claude-opus-4-7",
    "claude-opus-4@",
];

/// Whether this Claude model accepts a mid-conversation system turn.
/// Unknown/future models default to `true` — the pre-4.8 set is closed, new
/// models all support it.
pub fn supports_mid_conv_system(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    !PRE_OPUS_48.iter().any(|p| m.contains(p))
}

#[cfg(test)]
mod tests {
    use super::supports_mid_conv_system;

    #[test]
    fn pre_opus_48_downgrades_and_newer_keeps() {
        for m in [
            "claude-3-5-sonnet-20241022",
            "claude-sonnet-4-5",
            "claude-haiku-4-5-20251001",
            "claude-opus-4-20250514",
            "claude-opus-4-1@20250805",
            "claude-opus-4-7",
            "us.anthropic.claude-sonnet-4-20250514-v1:0",
        ] {
            assert!(!supports_mid_conv_system(m), "{m} should downgrade");
        }
        for m in [
            "claude-opus-4-8",
            "claude-opus-4-9",
            "claude-fable-5",
            "claude-sonnet-5",
        ] {
            assert!(supports_mid_conv_system(m), "{m} should keep mid_conv");
        }
    }
}
