//! Normalized usage domain (§17): one canonical shape across protocols.
//! input = NON-cached input; cache fields separate; totals always computed.

pub mod extract;

use std::collections::BTreeMap;
use std::fmt;

use rust_decimal::Decimal;

/// Canonical token usage, normalized across provider families.
///
/// `input` counts only NON-cached input tokens; cache reads/creations are
/// recorded in their own columns. Upstream-reported totals are never trusted —
/// [`NormalizedUsage::total`] recomputes from the parts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedUsage {
    pub input: u64,
    pub output: u64,
    /// Image-output tokens, separated from ordinary text/reasoning output so
    /// providers with a distinct image-output rate are not double-billed.
    pub image_output: u64,
    pub cache_read: u64,
    pub cache_creation_5m: u64,
    pub cache_creation_30m: u64,
    pub cache_creation_1h: u64,
    /// Informational subset of `output` (already billed there).
    pub reasoning: u64,
    /// Non-token billable quantities (seconds, characters, requests, images,
    /// search units, video tokens, and future provider metrics).
    pub metrics: BTreeMap<String, Decimal>,
    /// Conditions used to select dimensional rates (resolution, mode, audio,
    /// provider SKU, and similar categorical attributes).
    pub dimensions: BTreeMap<String, String>,
}

impl NormalizedUsage {
    pub fn cache_creation(&self) -> u64 {
        self.cache_creation_5m + self.cache_creation_30m + self.cache_creation_1h
    }

    pub fn total(&self) -> u64 {
        self.input + self.output + self.image_output + self.cache_read + self.cache_creation()
    }

    pub fn metric(&self, name: &str) -> Decimal {
        self.metrics.get(name).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn set_metric(&mut self, name: impl Into<String>, quantity: Decimal) {
        self.metrics.insert(name.into(), quantity);
    }
}

/// Where the recorded usage came from (DB string column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSource {
    Upstream,
    Counted,
    Estimated,
}

impl UsageSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::Counted => "counted",
            Self::Estimated => "estimated",
        }
    }
}

impl fmt::Display for UsageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How the response ended (DB string column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Complete,
    Interrupted,
}

impl Ended {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Interrupted => "interrupted",
        }
    }
}

impl fmt::Display for Ended {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
