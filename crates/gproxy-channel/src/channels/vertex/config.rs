//! Provider `config` JSON this channel decodes. Unknown keys are ignored.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;

/// Google's default region when nothing names one.
pub(super) const DEFAULT_LOCATION: &str = "us-central1";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct VertexConfig {
    /// Google Cloud project id. Falls back to the service-account key's own
    /// `project_id`, which is the usual case: one key, one project.
    pub project: Option<String>,
    /// Vertex region, for example `us-central1`, `europe-west4` or `global`.
    /// The region decides the origin as well as the path.
    pub location: Option<String>,
}

impl VertexConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

/// A configured or secret-held string with surrounding space and empty
/// strings removed, so a form that wrote `""` reads the same as one that
/// wrote nothing.
pub(super) fn setting(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
