//! The provider `config` JSON this channel decodes. Unknown keys are ignored
//! so a provider row can carry host-owned keys next to these.

use serde::Deserialize;

use crate::channel::{ChannelError, ProviderView};

/// The region AWS gives every account by default.
pub const DEFAULT_REGION: &str = "us-east-1";
/// The `anthropic_version` Bedrock's InvokeModel envelope expects for
/// Anthropic models; it is not the anthropic.com Messages version.
pub const DEFAULT_ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct BedrockConfig {
    /// The AWS region whose regional endpoints are addressed and whose name
    /// enters the SigV4 credential scope.
    pub region: String,
    /// Origin of the control plane (`bedrock.{region}.amazonaws.com`), which
    /// serves the foundation-model directory. The provider's own `base_url`
    /// column overrides the runtime plane instead.
    pub control_base_url: Option<String>,
    /// The Bedrock envelope version written into every Anthropic request body.
    pub anthropic_version: String,
    /// Place `cache_control` where a client embeds a magic cache string in
    /// the Claude body. The strings are stripped either way.
    pub enable_claude_magic_cache: bool,
}

impl Default for BedrockConfig {
    fn default() -> Self {
        Self {
            region: DEFAULT_REGION.into(),
            control_base_url: None,
            anthropic_version: DEFAULT_ANTHROPIC_VERSION.into(),
            enable_claude_magic_cache: false,
        }
    }
}

impl BedrockConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        let mut config: Self = serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
        config.region = config.region.trim().to_owned();
        if config.region.is_empty() {
            config.region = DEFAULT_REGION.into();
        }
        config.anthropic_version = config.anthropic_version.trim().to_owned();
        if config.anthropic_version.is_empty() {
            config.anthropic_version = DEFAULT_ANTHROPIC_VERSION.into();
        }
        Ok(config)
    }

    /// The region, rejected unless it looks like one: it is interpolated into
    /// a hostname and into the signature's credential scope.
    pub fn region(&self) -> Result<&str, ChannelError> {
        if !self.region.is_empty()
            && self
                .region
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            Ok(&self.region)
        } else {
            Err(ChannelError::InvalidConfig(format!(
                "`{}` is not an AWS region",
                self.region
            )))
        }
    }
}
