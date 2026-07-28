//! Root-only built-in TLS/HTTP impersonation factories.

use crate::channel::bulletins;

pub type EmulationFactory = fn() -> wreq::Emulation;

pub fn builtin() -> Vec<(&'static str, EmulationFactory)> {
    vec![
        #[cfg(feature = "channel-geminicli")]
        ("geminicli", bulletins::geminicli::default_emulation),
        #[cfg(feature = "channel-antigravity")]
        ("antigravity", bulletins::antigravity::default_emulation),
        #[cfg(feature = "channel-codex")]
        ("codex", bulletins::codex::default_emulation),
        #[cfg(feature = "channel-claudecode")]
        ("claudecode", bulletins::claudecode::default_emulation),
        #[cfg(feature = "channel-kiro")]
        ("kiro", bulletins::kiro::default_emulation),
        #[cfg(feature = "channel-copilotcli")]
        ("copilotcli", bulletins::copilotcli::default_emulation),
        #[cfg(feature = "channel-claudeweb")]
        ("claudeweb", bulletins::claudeweb::default_emulation),
    ]
}

#[cfg(test)]
mod tests {
    use crate::http::client::WreqClient;

    #[test]
    fn builtin_emulations_build() {
        for (id, factory) in super::builtin() {
            WreqClient::with_proxy_and_emulation(None, Some(factory()))
                .unwrap_or_else(|error| panic!("{id}: default emulation did not build: {error}"));
        }
    }
}
