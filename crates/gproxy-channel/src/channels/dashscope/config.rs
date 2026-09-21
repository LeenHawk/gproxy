//! Provider configuration and upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "dashscope";
pub const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com";
/// The OpenAI-compatible mode.
pub(super) const COMPATIBLE_MODE: &str = "/compatible-mode";
/// The Anthropic-compatible mode, which is not under the one above.
pub(super) const ANTHROPIC_MODE: &str = "/apps/anthropic";
/// Reranking, on a prefix of its own.
pub(super) const RERANK_PATH: &str = "/compatible-api/v1/reranks";

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DashScopeConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl DashScopeConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
