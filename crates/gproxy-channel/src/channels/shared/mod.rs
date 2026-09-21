//! Code the concrete channels share. Policy stays in each channel; these
//! modules only execute what a channel asks for.

#[cfg(any(
    feature = "codex",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "custom"
))]
pub(crate) mod cache;
#[cfg(any(feature = "codex", feature = "claudecode"))]
pub(crate) mod services_common;
#[cfg(any(feature = "azure", feature = "vertex", feature = "vertexexpress"))]
pub(crate) mod vendor_usage;
