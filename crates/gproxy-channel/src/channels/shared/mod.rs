//! Code the concrete channels share. Policy stays in each channel; these
//! modules only execute what a channel asks for.

/// The magic cache strings, for the dialects that have cache breakpoints.
#[cfg(any(
    feature = "claudeapi",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "codex",
    feature = "custom",
    feature = "openai"
))]
pub(crate) mod cache;
/// The OpenAI request and usage wire the OpenAI-compatible channels share.
#[cfg(any(feature = "aistudio", feature = "claudeapi", feature = "openai"))]
pub(crate) mod openai_wire;
#[cfg(any(feature = "codex", feature = "claudecode"))]
pub(crate) mod services_common;
#[cfg(any(feature = "azure", feature = "vertex", feature = "vertexexpress"))]
pub(crate) mod vendor_usage;
