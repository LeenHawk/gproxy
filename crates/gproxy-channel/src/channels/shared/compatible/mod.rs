//! Mechanics the API-key fleet shares: bounded ability calls and the
//! authentication plumbing of their HTTP requests.
//!
//! Every upstream in this family answers Chat Completions, Responses or
//! Claude Messages verbatim, so the host reads their usage the standard way;
//! a vendor's own extra fields are that channel's `UsageExtras`. Policy
//! stays in each channel; this module only executes what a channel asks for.

/// Only the channels with a quota probe or a token exchange compile the
/// ability helpers.
#[cfg(any(
    feature = "cline",
    feature = "cloudflare_ai_gateway",
    feature = "copilotcli",
    feature = "deepseek",
    feature = "glm",
    feature = "minimax",
    feature = "grokbuild",
    feature = "kimi",
    feature = "opencode",
    feature = "openrouter",
    feature = "vercel",
    feature = "xai"
))]
pub(crate) mod ability;
pub(crate) mod http;
