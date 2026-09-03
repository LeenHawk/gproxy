use serde::{Deserialize, Serialize};

use super::Modality;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModalityTokenCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modality: Option<Modality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<i64>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
