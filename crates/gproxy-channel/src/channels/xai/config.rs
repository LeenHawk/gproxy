//! Provider configuration and upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "xai";
pub const DEFAULT_BASE_URL: &str = "https://api.x.ai";
/// Billing lives on a different host from inference, behind a different key.
pub const DEFAULT_MANAGEMENT_URL: &str = "https://management-api.x.ai";

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct XaiConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
    /// An xAI *management* key, which is not the inference key. Without it
    /// there is no billing surface to ask.
    pub quota_api_key: Option<String>,
    /// The team whose billing the probe reads.
    pub quota_team_id: Option<String>,
    /// Management origin; defaults to https://management-api.x.ai.
    pub quota_base_url: Option<String>,
}

impl XaiConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// The management key and team, when both are configured.
    pub(super) fn billing(&self) -> Option<(&str, &str)> {
        let key = self
            .quota_api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())?;
        let team = self
            .quota_team_id
            .as_deref()
            .map(str::trim)
            .filter(|team| !team.is_empty())?;
        Some((key, team))
    }

    pub(super) fn management_url(&self) -> &str {
        self.quota_base_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .unwrap_or(DEFAULT_MANAGEMENT_URL)
            .trim_end_matches('/')
    }
}
