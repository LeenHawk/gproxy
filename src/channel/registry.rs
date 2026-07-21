//! Startup-built `channel_id -> Arc<dyn Channel>` map (§6.3). No big match.
//!
//! Each channel is a folder under [`crate::channel::bulletins`] that manages its
//! own auth (`auth.rs`). The id (== `Provider.channel`) is the registry key.
//! Built-in channels are functional — API-key, OAuth (`refresh_token` grant /
//! SA-JWT / device-token), and the Code-Assist / Smithy envelope channels all
//! build real upstream requests (M7a/M7b landed the OAuth infra + transforms).

use std::collections::HashMap;
use std::sync::Arc;

use crate::channel::bulletins;
use crate::channel::{Channel, ChannelLogin};

/// Registry of channel adapters keyed by `Channel::id` (== `Provider.channel`).
///
/// `login` is a parallel map holding the channels that support a §14.5
/// interactive login (authcode: codex, claudecode, geminicli, antigravity,
/// kiro; device-code: grokbuild, copilotcli; cookie: claudecode,
/// claudeweb, chatgpt); a channel absent from it has no login flow.
pub struct ChannelRegistry {
    map: HashMap<&'static str, Arc<dyn Channel>>,
    login: HashMap<&'static str, Arc<dyn ChannelLogin>>,
}

impl ChannelRegistry {
    /// Build the full channel set. Pure `http` + `serde_json` logic; compiles on
    /// native AND wasm32.
    pub fn with_builtin() -> Self {
        let mut map: HashMap<&'static str, Arc<dyn Channel>> = HashMap::new();
        for ch in builtin_channels() {
            map.insert(ch.id(), ch);
        }
        let mut login: HashMap<&'static str, Arc<dyn ChannelLogin>> = HashMap::new();
        for (id, lg) in builtin_logins() {
            login.insert(id, lg);
        }
        Self { map, login }
    }

    /// Look up a channel by id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Channel>> {
        self.map.get(id).cloned()
    }

    /// Look up a channel's interactive login, or `None` if it has no login flow.
    pub fn login_for(&self, id: &str) -> Option<Arc<dyn ChannelLogin>> {
        self.login.get(id).cloned()
    }

    /// Test-only: build the full built-in set plus one extra (or overriding)
    /// channel under `id`. Lets integration tests drive paths no built-in
    /// channel exercises (e.g. a channel whose `refresh` succeeds).
    #[cfg(test)]
    pub fn with_channel(id: &'static str, channel: Arc<dyn Channel>) -> Self {
        let mut reg = Self::with_builtin();
        reg.map.insert(id, channel);
        reg
    }
}

/// All built-in channel adapters (all functional as of M7b).
fn builtin_channels() -> Vec<Arc<dyn Channel>> {
    vec![
        // ── API-key ──
        #[cfg(feature = "channel-openai")]
        Arc::new(bulletins::openai::OpenAiChannel),
        #[cfg(feature = "channel-openrouter")]
        Arc::new(bulletins::openrouter::OpenRouterChannel),
        #[cfg(feature = "channel-deepseek")]
        Arc::new(bulletins::deepseek::DeepSeekChannel),
        #[cfg(feature = "channel-groq")]
        Arc::new(bulletins::groq::GroqChannel),
        #[cfg(feature = "channel-nvidia")]
        Arc::new(bulletins::nvidia::NvidiaChannel),
        #[cfg(feature = "channel-vercel")]
        Arc::new(bulletins::vercel::VercelChannel),
        #[cfg(feature = "channel-custom")]
        Arc::new(bulletins::custom::CustomChannel),
        #[cfg(feature = "channel-claudeapi")]
        Arc::new(bulletins::claudeapi::ClaudeApiChannel),
        #[cfg(feature = "channel-aistudio")]
        Arc::new(bulletins::aistudio::AiStudioChannel),
        #[cfg(feature = "channel-vertexexpress")]
        Arc::new(bulletins::vertexexpress::VertexExpressChannel),
        // ── OAuth / envelope ──
        #[cfg(feature = "channel-vertex")]
        Arc::new(bulletins::vertex::VertexChannel),
        #[cfg(feature = "channel-geminicli")]
        Arc::new(bulletins::geminicli::GeminiCliChannel),
        #[cfg(feature = "channel-antigravity")]
        Arc::new(bulletins::antigravity::AntigravityChannel),
        #[cfg(feature = "channel-grokbuild")]
        Arc::new(bulletins::grokbuild::GrokBuildChannel),
        #[cfg(feature = "channel-claudecode")]
        Arc::new(bulletins::claudecode::ClaudeCodeChannel),
        #[cfg(feature = "channel-codex")]
        Arc::new(bulletins::codex::CodexChannel),
        #[cfg(feature = "channel-kiro")]
        Arc::new(bulletins::kiro::KiroChannel),
        #[cfg(feature = "channel-copilotcli")]
        Arc::new(bulletins::copilotcli::CopilotCliChannel),
        #[cfg(feature = "channel-chatgpt")]
        Arc::new(bulletins::chatgpt::ChatGptChannel),
        #[cfg(all(feature = "channel-claudeweb", not(target_arch = "wasm32")))]
        Arc::new(bulletins::claudeweb::ClaudeWebChannel),
        #[cfg(all(feature = "channel-tasklet", not(target_arch = "wasm32")))]
        Arc::new(bulletins::tasklet::TaskletChannel),
    ]
}

/// Channels that support a §14.5 interactive login, paired with `Channel::id`.
/// Authcode, device-code, and cookie-capable channels all live here.
fn builtin_logins() -> Vec<(&'static str, Arc<dyn ChannelLogin>)> {
    vec![
        #[cfg(feature = "channel-codex")]
        ("codex", Arc::new(bulletins::codex::CodexChannel)),
        #[cfg(feature = "channel-claudecode")]
        (
            "claudecode",
            Arc::new(bulletins::claudecode::ClaudeCodeChannel),
        ),
        #[cfg(feature = "channel-geminicli")]
        (
            "geminicli",
            Arc::new(bulletins::geminicli::GeminiCliChannel),
        ),
        #[cfg(feature = "channel-antigravity")]
        (
            "antigravity",
            Arc::new(bulletins::antigravity::AntigravityChannel),
        ),
        #[cfg(feature = "channel-grokbuild")]
        (
            "grokbuild",
            Arc::new(bulletins::grokbuild::GrokBuildChannel),
        ),
        #[cfg(feature = "channel-kiro")]
        ("kiro", Arc::new(bulletins::kiro::KiroChannel)),
        #[cfg(feature = "channel-copilotcli")]
        (
            "copilotcli",
            Arc::new(bulletins::copilotcli::CopilotCliChannel),
        ),
        #[cfg(feature = "channel-chatgpt")]
        ("chatgpt", Arc::new(bulletins::chatgpt::ChatGptChannel)),
        #[cfg(all(feature = "channel-claudeweb", not(target_arch = "wasm32")))]
        (
            "claudeweb",
            Arc::new(bulletins::claudeweb::ClaudeWebChannel),
        ),
    ]
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::with_builtin()
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod emulation_tests {
    use super::builtin_channels;
    use crate::http::client::WreqClient;

    /// Every impersonation channel's opt-in built-in `default_emulation` must
    /// build a real wreq client. BoringSSL validates the cipher/curve/sigalg
    /// token strings only at client-build time (not `TlsOptions::build`), so
    /// this is the test that catches a bad token in a channel's `fingerprint.rs`.
    #[test]
    fn channel_default_emulations_build() {
        let expected = [
            "claudecode",
            "codex",
            "geminicli",
            "antigravity",
            "kiro",
            "copilotcli",
            "chatgpt",
            "claudeweb",
        ];
        let mut found = Vec::new();
        for ch in builtin_channels() {
            if let Some(emu) = ch.default_emulation() {
                WreqClient::with_proxy_and_emulation(None, Some(emu)).unwrap_or_else(|e| {
                    panic!("{}: default_emulation client build failed: {e}", ch.id())
                });
                found.push(ch.id());
            }
        }
        for id in expected {
            assert!(
                found.contains(&id),
                "{id} should expose a default_emulation"
            );
        }
    }
}
