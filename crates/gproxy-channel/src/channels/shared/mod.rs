//! Code the concrete channels share. Policy stays in each channel; these
//! modules only execute what a channel asks for.
//!
//! Three modules here read a `usage` object, and they stay three on purpose.
//! `openai_wire`, `vendor_usage` and `compatible::usage` agree on the token
//! arithmetic of a well-formed OpenAI body and disagree about everything
//! around it: which dialects exist, what an absent count means, which of the
//! vendor's extra fields are metered, and whether the answer arrives buffered
//! or through an observer. Parameterizing those differences into one reader
//! would put every channel's billing behind one set of flags — see
//! `design/transform.md` on why this repository keeps vendor-native readings
//! apart rather than routing them through a pivot.

/// AWS `vnd.amazon.eventstream` framing, for the upstreams that answer in it
/// rather than in SSE. Shared by `aws_bedrock` and `kiro`; each keeps its own
/// translator, because what a decoded payload means is not the framing's
/// business.
#[cfg(any(feature = "aws_bedrock", feature = "kiro"))]
pub(crate) mod aws_eventstream;
#[cfg(any(feature = "aws_bedrock", feature = "kiro"))]
pub(crate) mod aws_reason;
/// The magic cache strings, for the dialects that have cache breakpoints.
#[cfg(any(
    feature = "azure",
    feature = "aws_bedrock",
    feature = "claudeapi",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "codex",
    feature = "custom",
    feature = "openai",
    feature = "opencode",
    feature = "openrouter",
    feature = "vercel"
))]
pub(crate) mod cache;
/// The Code Assist envelope and Google login the Gemini CLI channels share.
#[cfg(any(feature = "antigravity", feature = "geminicli"))]
pub(crate) mod code_assist;
/// Bounded ability calls, and usage for the API-key fleet: Chat, Responses and
/// Claude Messages keyed by dialect, with each channel's own fields read by an
/// `Enrich` hook it passes in. Shared by `cline`, `copilotcli`, `dashscope`,
/// `deepseek`, `grokbuild`, `kimi`, `opencode`, `openrouter` and `xai`.
#[cfg(any(
    feature = "aws_bedrock",
    feature = "cloudflare_ai_gateway",
    feature = "nvidia",
    feature = "cline",
    feature = "copilotcli",
    feature = "dashscope",
    feature = "deepseek",
    feature = "grokbuild",
    feature = "kimi",
    feature = "opencode",
    feature = "openrouter",
    feature = "vercel",
    feature = "xai"
))]
pub(crate) mod compatible;
/// The OpenAI platform's own wire: the streaming usage opt-in on the request,
/// and metering out of Chat, Responses, image and transcription replies with
/// the extras only that platform reports — the `cache_write_tokens` bucket,
/// audio token counts, web-search calls and `service_tier`. Takes no dialect;
/// it recognizes a usage object by its field names. Shared by `aistudio`,
/// `claudeapi`, `kiro`, `openai` and `workbuddy`, each of which gates it on
/// `WireFamily::OpenAi`.
#[cfg(any(
    feature = "aistudio",
    feature = "claudeapi",
    feature = "kiro",
    feature = "openai",
    feature = "workbuddy"
))]
pub(crate) mod openai_wire;
/// Route classification, local JSON answers, synthetic ids and the binding
/// mechanics behind resource routes, for the channels that serve a vendor's
/// own service surface. Shared by `claudecode` and `codex`.
#[cfg(any(feature = "codex", feature = "claudecode"))]
pub(crate) mod services_common;
/// Usage as the four vendor wires report it — OpenAI Responses, OpenAI Chat,
/// Claude Messages and Gemini `usageMetadata` — for the channels that forward
/// a vendor's body unchanged and meter it after the fact, buffered JSON or
/// accumulated SSE alike. Shared by `azure`, `custom`, `vertex` and
/// `vertexexpress`: three resell a named vendor, and `custom` points at
/// whichever one an operator configured, which is why it needs the reader that
/// covers all four and answers `None` for anything else.
#[cfg(any(
    feature = "cloudflare_ai_gateway",
    feature = "nvidia",
    feature = "azure",
    feature = "custom",
    feature = "vercel",
    feature = "vertex",
    feature = "vertexexpress"
))]
pub(crate) mod vendor_usage;

#[cfg(any(feature = "claudecode", feature = "claudeweb"))]
pub(crate) fn browser_connection() -> gproxy_client::ConnectionConfig {
    use gproxy_client::{Backend, ConnectionConfig, EmulationConfig};

    ConnectionConfig {
        backend: Backend::Wreq,
        emulation: Some(EmulationConfig::Preset {
            profile: "chrome_149".into(),
            platform: host_platform().into(),
            http2: true,
            headers: true,
        }),
        gzip: true,
        brotli: true,
        deflate: true,
        zstd: true,
        ..ConnectionConfig::default()
    }
}

#[cfg(any(feature = "claudecode", feature = "claudeweb"))]
/// wreq-util's platform name for the host (`std::env::consts::OS`).
fn host_platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

#[cfg(any(
    feature = "claudeapi",
    feature = "claudecode",
    feature = "custom",
    feature = "openrouter",
    feature = "vercel"
))]
pub(crate) mod claude_fallback;

#[cfg(any(feature = "claudeapi", feature = "vercel"))]
pub(crate) mod claude_hygiene;
