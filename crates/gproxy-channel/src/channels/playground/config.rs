use gproxy_upstream::{ProviderView, UpstreamError};
use serde::Deserialize;

pub const DEFAULT_BASE_URL: &str = "https://alkalimakersuite-pa.clients6.google.com";
pub(super) const RPC_PATH: &str =
    "/$rpc/google.internal.alkali.applications.makersuite.v1.MakerSuiteService";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PlaygroundConfig {
    pub timezone: String,
}

impl Default for PlaygroundConfig {
    fn default() -> Self {
        Self {
            timezone: "UTC".into(),
        }
    }
}

impl PlaygroundConfig {
    pub(super) fn parse(provider: ProviderView<'_>) -> Result<Self, UpstreamError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|_| UpstreamError::InvalidConfig("invalid Playground configuration".into()))
    }
}

pub(super) fn endpoint(
    provider: ProviderView<'_>,
    method: &str,
    override_url: Option<&str>,
) -> String {
    override_url.map(str::to_owned).unwrap_or_else(|| {
        let base = provider
            .base_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/');
        format!("{base}{RPC_PATH}/{method}")
    })
}
