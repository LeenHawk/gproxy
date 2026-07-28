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
use crate::channel::registration::RegisteredChannel;
use crate::channel::{Channel, ChannelLogin};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelRegistryError {
    #[error("channel `{0}` is already registered")]
    DuplicateChannel(&'static str),
    #[error("channel login `{0}` is already registered")]
    DuplicateLogin(&'static str),
}

/// Registry of channel adapters keyed by `Channel::id` (== `Provider.channel`).
///
/// `login` is a parallel map holding the channels that support a §14.5
/// interactive login (authcode: codex, claudecode, geminicli, antigravity,
/// kiro; device-code: grokbuild, copilotcli; cookie: claudecode, claudeweb); a
/// channel absent from it has no login flow.
pub struct ChannelRegistry {
    map: HashMap<&'static str, Arc<dyn Channel>>,
    login: HashMap<&'static str, Arc<dyn ChannelLogin>>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    emulation: HashMap<&'static str, crate::channel::emulation::EmulationFactory>,
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
        #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
        let emulation = crate::channel::emulation::builtin().into_iter().collect();
        Self {
            map,
            login,
            #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
            emulation,
        }
    }

    /// Build the built-in registry and apply compile-time external
    /// registrations. External registration is native-only and feature-gated;
    /// without it this is identical to [`with_builtin`](Self::with_builtin).
    pub fn with_builtin_and_linked() -> Result<Self, ChannelRegistryError> {
        let registry = Self::with_builtin();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut registry = registry;
            for constructor in crate::channel::registration::CHANNEL_REGISTRATIONS {
                registry.register(constructor())?;
            }
            return Ok(registry);
        }
        #[cfg(target_arch = "wasm32")]
        Ok(registry)
    }

    /// Look up a channel by id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Channel>> {
        self.map.get(id).cloned()
    }

    /// Look up a channel's interactive login, or `None` if it has no login flow.
    pub fn login_for(&self, id: &str) -> Option<Arc<dyn ChannelLogin>> {
        self.login.get(id).cloned()
    }

    /// Resolve a root-owned built-in TLS/HTTP impersonation profile.
    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    pub fn default_emulation(&self, id: &str) -> Option<wreq::Emulation> {
        self.emulation.get(id).map(|factory| factory())
    }

    /// Register one externally supplied channel and its optional login adapter.
    /// Registration is startup-only; duplicate IDs are rejected rather than
    /// depending on linker order or silently replacing a built-in channel.
    pub fn register(
        &mut self,
        registration: RegisteredChannel,
    ) -> Result<(), ChannelRegistryError> {
        let id = registration.channel.id();
        if self.map.contains_key(id) {
            return Err(ChannelRegistryError::DuplicateChannel(id));
        }
        if registration.login.is_some() && self.login.contains_key(id) {
            return Err(ChannelRegistryError::DuplicateLogin(id));
        }
        self.map.insert(id, registration.channel);
        if let Some(login) = registration.login {
            self.login.insert(id, login);
        }
        Ok(())
    }
}

/// All built-in channel adapters (all functional as of M7b).
fn builtin_channels() -> Vec<Arc<dyn Channel>> {
    vec![
        // ── API-key ──
        #[cfg(feature = "channel-openai")]
        Arc::new(bulletins::openai::OpenAiChannel),
        #[cfg(feature = "channel-azure")]
        Arc::new(bulletins::azure::AzureChannel),
        #[cfg(feature = "channel-aws-bedrock")]
        Arc::new(bulletins::aws_bedrock::AwsBedrockChannel),
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
        #[cfg(all(feature = "channel-claudeweb", not(target_arch = "wasm32")))]
        Arc::new(bulletins::claudeweb::ClaudeWebChannel),
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
