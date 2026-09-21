//! Provider `config` JSON this channel decodes. Unknown keys are ignored.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AzureConfig {
    /// Azure OpenAI resource name: the origin becomes
    /// `https://{resource}.openai.azure.com`. Ignored when the provider row
    /// carries a `base_url`, which a Foundry project or private endpoint has.
    pub resource: Option<String>,
    /// `api-version` query value. Required together with `deployment`; on the
    /// v1 surface it is optional and only sent when configured.
    pub api_version: Option<String>,
    /// Deployment name. Setting it selects the deployment-scoped layout
    /// `/openai/deployments/{deployment}/…` instead of the v1 surface.
    pub deployment: Option<String>,
}

impl AzureConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

/// A configured string with surrounding space and empty strings removed, so a
/// form that wrote `""` reads the same as one that wrote nothing.
pub(super) fn setting(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
