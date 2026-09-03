//! OpenAI-shaped rerank compatibility wire.
//!
//! OpenAI has no standalone rerank endpoint in the local documentation
//! snapshot. This shape is retained from v2 for compatible providers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{OpenAiModelId, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RerankRequest {
    pub model: OpenAiModelId,
    pub query: String,
    pub documents: Vec<RerankDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruct: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_documents: Option<bool>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RerankDocument {
    Text(String),
    Structured(RerankDocumentContent),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RerankDocumentContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RerankResponse {
    pub model: OpenAiModelId,
    pub results: Vec<RerankResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<RerankUsage>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RerankResult {
    pub index: u32,
    pub relevance_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<RerankDocumentContent>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RerankUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_units: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rerank_round_trip_preserves_unknown_fields_and_documents() {
        let value = json!({
            "model":"rerank-future",
            "query":"q",
            "documents":["text", {"future_document":true}],
            "future_request":{"x":1}
        });
        let parsed: RerankRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
}
