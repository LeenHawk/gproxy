use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImagenPredictRequest {
    // models.predict defines each instance as google.protobuf.Value.
    pub instances: Vec<Value>,
    /// `upstream_docs/gemini/docs/Models.md`, `models.predict.parameters`:
    /// prediction parameters are an arbitrary `google.protobuf.Value`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImagenPredictResponse {
    // models.predict defines each prediction as google.protobuf.Value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predictions: Vec<Value>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, Value>,
}
