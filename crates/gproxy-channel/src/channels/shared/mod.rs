//! Code the concrete channels share. Policy stays in each channel; these
//! modules only execute what a channel asks for.

/// AWS `vnd.amazon.eventstream` framing, for the upstreams that answer in it
/// rather than in SSE. `aws_bedrock` carries an equivalent private copy in its
/// own `stream.rs`; folding the two together means touching that channel, so
/// it waits for a wave that owns it.
#[cfg(feature = "kiro")]
pub(crate) mod aws_eventstream;
/// The magic cache strings, for the dialects that have cache breakpoints.
#[cfg(any(
    feature = "claudeapi",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "codex",
    feature = "custom",
    feature = "opencode",
    feature = "openrouter"
))]
pub(crate) mod cache;
/// The Code Assist envelope and Google login the Gemini CLI channels share.
#[cfg(any(feature = "antigravity", feature = "geminicli"))]
pub(crate) mod code_assist;
/// Bounded ability calls and streaming usage for the API-key fleet.
#[cfg(any(
    feature = "cline",
    feature = "copilotcli",
    feature = "dashscope",
    feature = "deepseek",
    feature = "grokbuild",
    feature = "kimi",
    feature = "opencode",
    feature = "openrouter",
    feature = "xai"
))]
pub(crate) mod compatible;
/// The OpenAI request and usage wire the OpenAI-compatible channels share.
#[cfg(any(
    feature = "aistudio",
    feature = "claudeapi",
    feature = "kiro",
    feature = "openai",
    feature = "workbuddy"
))]
pub(crate) mod openai_wire;
#[cfg(any(feature = "codex", feature = "claudecode"))]
pub(crate) mod services_common;
#[cfg(any(feature = "azure", feature = "vertex", feature = "vertexexpress"))]
pub(crate) mod vendor_usage;
