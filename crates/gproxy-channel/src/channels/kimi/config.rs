//! Provider configuration and upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "kimi";
/// The Moonshot platform, which an API key buys access to.
pub const DEFAULT_API_BASE_URL: &str = "https://api.moonshot.cn";
/// The Kimi Code subscription, which a device login buys access to. The
/// origin already carries `/coding/v1`, so native paths drop their `/v1`.
pub const DEFAULT_CODE_BASE_URL: &str = "https://api.kimi.com/coding/v1";
pub const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
pub const DEFAULT_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
pub const DEFAULT_CLI_VERSION: &str = "0.36.1";
/// The platform name the CLI announces itself under.
pub const CLI_PLATFORM: &str = "kimi_code_cli";

/// Which of Moonshot's two products a provider row fronts. The credential
/// decides how a request authenticates; this decides what the upstream can be
/// asked, which is a question the host puts before any credential is bound.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Product {
    /// api.moonshot.cn, an OpenAI-compatible platform reached with an API key.
    #[default]
    Platform,
    /// api.kimi.com/coding/v1, the Kimi Code subscription, which also answers
    /// Claude Messages on its own wire.
    Code,
}

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct KimiConfig {
    /// Stated by the operator; inferred from `base_url` when absent.
    pub product: Option<Product>,
    /// Where the device login and the refresh talk.
    pub oauth_host: String,
    pub client_id: String,
    /// The Kimi Code CLI version announced in `user-agent` and `x-msh-version`.
    pub cli_version: String,
    /// The machine the CLI claims to run on. v3 read the host environment;
    /// preparation here is pure, so the operator states it instead.
    pub device_name: Option<String>,
    /// Hardware description, `{os} {arch}` by default.
    pub device_model: Option<String>,
    /// Kernel or OS release, the build's target OS by default.
    pub os_version: Option<String>,
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl Default for KimiConfig {
    fn default() -> Self {
        Self {
            product: None,
            oauth_host: DEFAULT_OAUTH_HOST.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            cli_version: DEFAULT_CLI_VERSION.into(),
            device_name: None,
            device_model: None,
            os_version: None,
            headers: BTreeMap::new(),
        }
    }
}

impl KimiConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// The stated product, else what the configured origin says it is, else
    /// the platform. Guessing `Platform` is the safe way to be wrong: Claude
    /// Messages is then converted to Chat Completions, which both products
    /// answer, where the reverse would send `/v1/messages` to an origin that
    /// has no such path.
    pub(super) fn product(&self, provider: ProviderView<'_>) -> Product {
        if let Some(product) = self.product {
            return product;
        }
        let base = provider.base_url.unwrap_or_default();
        if base.contains("api.kimi.com") || base.contains("/coding") {
            Product::Code
        } else {
            Product::Platform
        }
    }

    pub(super) fn oauth_host(&self) -> &str {
        let host = self.oauth_host.trim().trim_end_matches('/');
        if host.is_empty() {
            DEFAULT_OAUTH_HOST
        } else {
            host
        }
    }

    pub(super) fn client_id(&self) -> &str {
        let id = self.client_id.trim();
        if id.is_empty() { DEFAULT_CLIENT_ID } else { id }
    }
}
