//! Mechanics the API-key fleet shares: bounded ability calls and usage
//! reading for the three compatible wire shapes.
//!
//! Every upstream in this family answers Chat Completions, Responses or
//! Claude Messages verbatim, so the usage object is one of three known
//! shapes and the only per-channel difference is the vendor's own extra
//! fields. Policy stays in each channel; this module only executes what a
//! channel asks for, through the `Enrich` hook it passes in.

/// Only the channels with a quota probe or a token exchange compile the
/// ability helpers.
#[cfg(any(
    feature = "cline",
    feature = "copilotcli",
    feature = "deepseek",
    feature = "grokbuild",
    feature = "kimi",
    feature = "opencode",
    feature = "openrouter",
    feature = "vercel",
    feature = "xai"
))]
pub(crate) mod ability;
pub(crate) mod http;
pub(crate) mod usage;
