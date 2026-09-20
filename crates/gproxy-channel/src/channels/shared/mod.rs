//! Code the concrete channels share. Policy stays in each channel; these
//! modules only execute what a channel asks for.

pub(crate) mod cache;
#[cfg(any(feature = "codex", feature = "claudecode"))]
pub(crate) mod services_common;
