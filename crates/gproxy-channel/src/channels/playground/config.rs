use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;

pub const DEFAULT_BASE_URL: &str = "https://alkalimakersuite-pa.clients6.google.com";
pub(super) const RPC_PATH: &str =
    "/$rpc/google.internal.alkali.applications.makersuite.v1.MakerSuiteService";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PlaygroundConfig {
    pub proof_server_url: String,
    pub timezone: String,
}

impl Default for PlaygroundConfig {
    fn default() -> Self {
        Self {
            proof_server_url: String::new(),
            timezone: "UTC".into(),
        }
    }
}

impl PlaygroundConfig {
    pub(super) fn parse(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|_| ChannelError::InvalidConfig("invalid Playground configuration".into()))
    }

    pub(super) fn proof_url(&self) -> Result<String, ChannelError> {
        let base = self.proof_server_url.trim().trim_end_matches('/');
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(ChannelError::InvalidConfig(
                "Playground requires your own proof_server_url".into(),
            ));
        }
        Ok(format!("{base}/v1/proof/mint"))
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
