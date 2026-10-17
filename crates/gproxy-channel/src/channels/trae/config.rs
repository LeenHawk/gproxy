use crate::channel::{ChannelError, CredentialView, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "trae";
pub const CLIENT_ID: &str = "ono9krqynydwx5";
pub const APP_ID: &str = "6eefa01c-1036-4c7e-9ca5-d891f63bfcd8";
pub const VERSION_CODE: &str = "20250325";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TraeConfig {
    pub international: bool,
    pub auth_base_url: Option<String>,
    pub ide_version: Option<String>,
    pub headers: BTreeMap<String, String>,
}

pub(super) fn non_empty(s: &str) -> Option<&str> {
    let s = s.trim();
    (!s.is_empty()).then_some(s)
}

pub(super) fn fact<'a>(credential: &'a CredentialView<'_>, key: &str) -> Option<&'a str> {
    credential
        .metadata
        .get(key)
        .or_else(|| {
            credential
                .secret
                .get("provider_fields")
                .and_then(|v| v.get(key))
        })
        .and_then(serde_json::Value::as_str)
        .and_then(non_empty)
}

pub(super) fn region(value: &str) -> &str {
    match value {
        "sg" | "Singapore" | "singapore" | "Singapore-Central" | "singapore-central" => "SG",
        "us" | "US-East" | "us-east" => "US",
        "usttp" => "USTTP",
        "cn" | "China" | "china" => "CN",
        other => other,
    }
}

impl TraeConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
    }

    pub(super) fn version(&self) -> &str {
        self.ide_version
            .as_deref()
            .and_then(non_empty)
            .unwrap_or(if self.international { "3.6.2" } else { "3.4.3" })
    }

    pub(super) fn build_version(&self) -> &str {
        if self.international {
            "2.3.91093"
        } else {
            "2.3.92182"
        }
    }

    pub(super) fn website(&self) -> &'static str {
        if self.international {
            "https://www.trae.ai"
        } else {
            "https://www.trae.cn"
        }
    }

    pub(super) fn api<'a>(
        &self,
        provider: ProviderView<'a>,
        credential: &CredentialView<'_>,
    ) -> &'a str {
        if let Some(base) = provider.base_url.and_then(non_empty) {
            return base.trim_end_matches('/');
        }
        if !self.international {
            return "https://trae-api-cn.mchost.guru";
        }
        match fact(credential, "region").map(region) {
            Some("SG") => "https://coresg-normal.trae.ai",
            Some("US") => "https://coreva-normal.trae.ai",
            Some("USTTP") => "https://core-normal.traeapi.us",
            _ => "https://core-normal.trae.ai",
        }
    }

    pub(super) fn auth_origin(&self, region: Option<&str>) -> &str {
        if let Some(base) = self.auth_base_url.as_deref().and_then(non_empty) {
            return base.trim_end_matches('/');
        }
        if !self.international {
            return "https://api.trae.cn";
        }
        match region.map(self::region) {
            Some("SG" | "US") => "https://growsg-normal.trae.ai",
            Some("USTTP") => "https://grow-normal.traeapi.us",
            _ => "https://grow-normal.trae.ai",
        }
    }

    pub(super) fn quota_origin(&self, region: Option<&str>) -> &str {
        if self.auth_base_url.as_deref().and_then(non_empty).is_some() || !self.international {
            return self.auth_origin(region);
        }
        if region.map(self::region) == Some("US") {
            "https://growva-normal.trae.ai"
        } else {
            self.auth_origin(region)
        }
    }
}
