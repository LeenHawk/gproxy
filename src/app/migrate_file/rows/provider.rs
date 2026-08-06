use serde::Deserialize;
use serde_json::Value;

use super::{default_json_object, default_true, default_weight};
use crate::store::persistence::records::{ProviderInput, ProviderModelInput};

#[derive(Deserialize)]
pub(crate) struct LegacyProvider {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_json_object")]
    pub settings_json: Value,
    #[serde(default = "default_strategy")]
    pub credential_strategy: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_strategy() -> String {
    "round_robin".to_owned()
}
fn default_kind() -> String {
    "api_key".to_owned()
}
impl From<LegacyProvider> for ProviderInput {
    fn from(x: LegacyProvider) -> Self {
        Self {
            id: Some(x.id),
            name: x.name,
            channel: x.channel,
            label: x.label,
            settings_json: x.settings_json,
            credential_strategy: x.credential_strategy,
            proxy_url: x.proxy_url,
            tls_fingerprint: x.tls_fingerprint,
            enabled: x.enabled,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyCredential {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub provider_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_json_object")]
    pub secret_json: Value,
    #[serde(default = "default_weight")]
    pub weight: i64,
    #[serde(default)]
    pub rpm_limit: Option<i64>,
    #[serde(default)]
    pub tpm_limit: Option<i64>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Deserialize)]
pub(crate) struct LegacyProviderModel {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub provider_id: i64,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub variants_json: Option<Value>,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub max_input_tokens: Option<i64>,
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyProviderModel> for ProviderModelInput {
    fn from(x: LegacyProviderModel) -> Self {
        Self {
            id: Some(x.id),
            provider_id: x.provider_id,
            model_id: x.model_id,
            display_name: x.display_name,
            variants_json: x.variants_json,
            context_window: x.context_window,
            max_input_tokens: x.max_input_tokens,
            max_output_tokens: x.max_output_tokens,
            enabled: x.enabled,
        }
    }
}
