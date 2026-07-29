//! Self-description used by the Admin API and generic Console forms.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::Provider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialFamily {
    ApiKey,
    OauthTokens,
    ServiceAccount,
    GithubToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginMode {
    Authcode,
    Device,
    Cookie,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingControl {
    Text,
    Url,
    Boolean,
    Integer,
    StringList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSettingField {
    pub key: String,
    pub control: SettingControl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMetadata {
    pub id: String,
    pub display_name: String,
    pub provider_family: Provider,
    pub credential_family: CredentialFamily,
    pub login_modes: Vec<LoginMode>,
    pub settings_fields: Vec<ChannelSettingField>,
    pub secret_template: Value,
    pub endpoint_kinds: Vec<String>,
    pub usage: bool,
}

impl ChannelMetadata {
    pub fn new(id: impl Into<String>, provider_family: Provider) -> Self {
        let id = id.into();
        Self {
            display_name: id.clone(),
            id,
            provider_family,
            credential_family: CredentialFamily::ApiKey,
            login_modes: Vec::new(),
            settings_fields: Vec::new(),
            secret_template: json!({ "api_key": "" }),
            endpoint_kinds: Vec::new(),
            usage: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelSource {
    Builtin,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCatalogEntry {
    pub source: ChannelSource,
    #[serde(flatten)]
    pub metadata: ChannelMetadata,
}
