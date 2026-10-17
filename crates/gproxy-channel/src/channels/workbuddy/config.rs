//! Provider `config` JSON for WorkBuddy. Unknown keys are ignored.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "workbuddy";
/// Mainland origin; the plugin API, auth and billing share one origin.
pub const DEFAULT_BASE_URL: &str = "https://copilot.tencent.com";
pub const INTERNATIONAL_BASE_URL: &str = "https://www.workbuddy.ai";
/// The plugin version the channel impersonates (v3 `identity.rs`).
pub const CLI_VERSION: &str = "4.22.16";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct WorkBuddyConfig {
    /// Selects WorkBuddy AI's origin and login platform. Defaults to mainland.
    pub international: bool,
    /// Replaces the plugin version in `x-ide-version` and in the user agent.
    pub ide_version: String,
    /// What the plugin says it is running inside. The CLI reports `CLI` for
    /// both the type and the name.
    pub ide_name: String,
    pub ide_type: String,
    /// The intent the agent surface is asked for; the CLI sends `craft`.
    pub agent_intent: String,
    /// Static headers added to every request.
    pub headers: BTreeMap<String, String>,
}

impl Default for WorkBuddyConfig {
    fn default() -> Self {
        Self {
            international: false,
            ide_version: CLI_VERSION.into(),
            ide_name: "CLI".into(),
            ide_type: "CLI".into(),
            agent_intent: "craft".into(),
            headers: BTreeMap::new(),
        }
    }
}

impl WorkBuddyConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    pub(super) fn base_url(&self, provider: ProviderView<'_>) -> String {
        provider
            .base_url
            .and_then(non_empty)
            .unwrap_or(if self.international {
                INTERNATIONAL_BASE_URL
            } else {
                DEFAULT_BASE_URL
            })
            .trim_end_matches('/')
            .to_owned()
    }

    pub(super) fn login_platform(&self) -> &'static str {
        if self.international {
            "workbuddy-ai"
        } else {
            "workbuddy"
        }
    }

    pub(super) fn ide_version(&self) -> &str {
        non_empty(&self.ide_version).unwrap_or(CLI_VERSION)
    }

    pub(super) fn user_agent(&self) -> String {
        format!("WorkBuddy/{}", self.ide_version())
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
