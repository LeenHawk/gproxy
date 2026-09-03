use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::common::{OpenAiModelId, OpenAiWireModel, Rest};
use super::generate_content::responses::ReasoningConfig;

pub type MemorySummarizeWireModel =
    OpenAiWireModel<MemorySummarizeRequest, MemorySummarizeResponse>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MemorySummarizeRequest {
    pub model: OpenAiModelId,
    pub traces: Vec<MemoryTrace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MemoryTrace {
    pub id: String,
    pub metadata: MemoryTraceMetadata,
    // No checked-in upstream memory schema exists; preserve proprietary trace items verbatim.
    pub items: Vec<Value>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MemoryTraceMetadata {
    pub source_path: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MemorySummarizeResponse {
    pub output: Vec<MemorySummary>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MemorySummary {
    #[serde(rename = "trace_summary", alias = "raw_memory")]
    pub trace_summary: String,
    pub memory_summary: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}
