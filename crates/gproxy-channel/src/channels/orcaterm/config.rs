use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "orcaterm";
pub const DEFAULT_BASE_URL: &str = "https://lightai.cloud.tencent.com";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct OrcaTermConfig {
    pub international: bool,
    pub site_url: Option<String>,
    pub oauth_provider: Option<String>,
    pub headers: BTreeMap<String, String>,
}

impl OrcaTermConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
    }

    pub(super) fn base_url<'a>(&self, provider: ProviderView<'a>) -> &'a str {
        provider
            .base_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(if self.international {
                "https://lightai.tencentcloud.com"
            } else {
                DEFAULT_BASE_URL
            })
            .trim_end_matches('/')
    }

    pub(super) fn site_url(&self) -> &str {
        self.site_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(if self.international {
                "https://orcaterm.ai"
            } else {
                "https://orcaterm.com"
            })
            .trim_end_matches('/')
    }

    pub(super) fn oauth_provider(&self) -> Result<&str, ChannelError> {
        let provider = self
            .oauth_provider
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(if self.international {
                "google"
            } else {
                "wechat"
            });
        match provider {
            "wechat" | "txcloud" | "google" | "github" => Ok(provider),
            _ => Err(ChannelError::InvalidConfig(
                "OrcaTerm oauth_provider must be wechat, txcloud, google or github".into(),
            )),
        }
    }
}

pub(super) fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("OrcaTerm: {}", message.into()))
}

pub(super) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub(super) fn new_id() -> Result<String, ChannelError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}
