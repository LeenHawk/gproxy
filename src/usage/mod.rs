//! Normalized usage domain (§17): one canonical shape across protocols.
//! input = NON-cached input; cache fields separate; totals always computed.

pub mod extract;

use std::fmt;

/// Canonical token usage, normalized across provider families.
///
/// `input` counts only NON-cached input tokens; cache reads/creations are
/// recorded in their own columns. Upstream-reported totals are never trusted —
/// [`NormalizedUsage::total`] recomputes from the parts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NormalizedUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    /// Informational subset of `output` (already billed there).
    pub reasoning: u64,
}

impl NormalizedUsage {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_creation
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
