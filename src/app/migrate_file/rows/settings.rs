use rust_decimal::Decimal;
use serde::Deserialize;

use super::{default_true, deserialize_decimal};
use crate::store::persistence::records::{InstanceSettingsInput, PriceRuleInput};

fn default_match_type() -> String {
    "exact".to_owned()
}

#[derive(Deserialize)]
pub(crate) struct LegacyPriceRule {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub provider_id: Option<i64>,
    #[serde(default = "default_match_type")]
    pub match_type: String,
    #[serde(default)]
    pub model_match: String,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub input_price: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub output_price: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub cache_read_price: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub cache_creation_5m_price: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub cache_creation_30m_price: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub cache_creation_1h_price: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub image_output_price: Decimal,
    #[serde(default)]
    pub pricing_tiers_json: Option<serde_json::Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyPriceRule> for PriceRuleInput {
    fn from(x: LegacyPriceRule) -> Self {
        Self {
            id: Some(x.id),
            provider_id: x.provider_id,
            match_type: x.match_type,
            model_match: x.model_match,
            input_price: x.input_price,
            output_price: x.output_price,
            cache_read_price: x.cache_read_price,
            cache_creation_5m_price: x.cache_creation_5m_price,
            cache_creation_30m_price: x.cache_creation_30m_price,
            cache_creation_1h_price: x.cache_creation_1h_price,
            image_output_price: x.image_output_price,
            pricing_tiers_json: x.pricing_tiers_json,
            rates: Vec::new(),
            enabled: x.enabled,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyInstanceSettings {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub instance_name: String,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub spoof_emulation: Option<bool>,
    #[serde(default)]
    pub enable_usage: bool,
    #[serde(default)]
    pub enable_upstream_log: bool,
    #[serde(default)]
    pub enable_upstream_log_body: bool,
    #[serde(default)]
    pub enable_downstream_log: bool,
    #[serde(default)]
    pub enable_downstream_log_body: bool,
    #[serde(default)]
    pub disable_log_redaction: bool,
    #[serde(default)]
    pub enable_tokenizer_download: bool,
    #[serde(default)]
    pub update_channel: Option<String>,
    #[serde(default)]
    pub enable_auto_update_check: bool,
    #[serde(default)]
    pub retention_days: Option<i64>,
}

impl From<LegacyInstanceSettings> for InstanceSettingsInput {
    fn from(x: LegacyInstanceSettings) -> Self {
        Self {
            id: Some(x.id),
            instance_name: x.instance_name,
            proxy: x.proxy,
            spoof_emulation: x.spoof_emulation,
            enable_usage: x.enable_usage,
            enable_upstream_log: x.enable_upstream_log,
            enable_upstream_log_body: x.enable_upstream_log_body,
            enable_downstream_log: x.enable_downstream_log,
            enable_downstream_log_body: x.enable_downstream_log_body,
            disable_log_redaction: x.disable_log_redaction,
            enable_tokenizer_download: x.enable_tokenizer_download,
            update_channel: x.update_channel,
            enable_auto_update_check: x.enable_auto_update_check,
            retention_days: x.retention_days,
            max_database_size_mb: None,
            file_upload_max_in_flight: 0,
        }
    }
}
