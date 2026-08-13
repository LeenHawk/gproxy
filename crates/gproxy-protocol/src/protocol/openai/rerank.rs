use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::{Extra, OpenAiModelId, OpenAiWireModel};

/// OpenAI-shaped rerank wire model used by compatible providers such as
/// OpenRouter and DashScope. OpenAI itself does not expose this endpoint.
pub type RerankWireModel = OpenAiWireModel<RerankRequest, RerankResponse>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
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
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum RerankDocument {
    Text(String),
    Structured(RerankDocumentContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RerankDocumentContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
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
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RerankResult {
    pub index: u32,
    pub relevance_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<RerankDocumentContent>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RerankUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_units: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_text_and_multimodal_documents() {
        let request: RerankRequest = serde_json::from_value(serde_json::json!({
            "model": "qwen3-vl-rerank",
            "query": "the red car",
            "documents": ["plain text", {"text": "a car", "image": "https://x/car.png"}],
            "top_n": 1,
            "provider": {"order": ["Alibaba"]}
        }))
        .unwrap();
        assert!(matches!(request.documents[0], RerankDocument::Text(_)));
        assert!(matches!(
            request.documents[1],
            RerankDocument::Structured(_)
        ));
        assert!(request.extra.contains_key("provider"));
    }
}
