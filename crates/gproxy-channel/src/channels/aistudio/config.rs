//! Provider configuration for the AI Studio channel.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "aistudio";
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
/// Where the OpenAI compatibility layer hangs off the same origin.
pub const OPENAI_PREFIX: &str = "/v1beta/openai";

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AistudioConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl AistudioConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
