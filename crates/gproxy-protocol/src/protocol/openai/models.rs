use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::*;

pub type ModelsWireModel = OpenAiWireModel<(), ModelListResponse>;
pub type ModelRetrieveWireModel = OpenAiWireModel<(), Model>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ModelListResponse {
    pub data: Vec<Model>,
    pub object: ListObjectType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct Model {
    pub id: OpenAiModelId,
    // OpenAI-compatible providers (e.g. DeepSeek) omit `created`; keep it
    // optional so decoding their model list for a response transform doesn't
    // fail on the missing field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    // gproxy extension: token limits surfaced from providers that report them
    // (Claude, Gemini). The official OpenAI model object has no such fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    pub object: ModelObjectType,
    pub owned_by: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DeepSeek (and other OpenAI-compatible providers) return model entries
    /// without `created`; decoding for a response transform must not fail.
    #[test]
    fn model_decodes_without_created() {
        let m: Model = serde_json::from_str(
            r#"{"id":"deepseek-chat","object":"model","owned_by":"deepseek"}"#,
        )
        .expect("decode without created");
        assert_eq!(m.created, None);
        // …and a missing `created` is omitted on re-encode, not fabricated.
        let s = serde_json::to_string(&m).unwrap();
        assert!(!s.contains("created"), "{s}");
    }
}
