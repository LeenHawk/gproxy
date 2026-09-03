use serde::{Deserialize, Serialize};

use super::UrlRetrievalStatus;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GroundingMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounding_chunks: Vec<GroundingChunk>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounding_supports: Vec<GroundingSupport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub web_search_queries: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_search_queries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_entry_point: Option<SearchEntryPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_metadata: Option<RetrievalMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_maps_widget_context_token: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SearchEntryPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_blob: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GroundingChunk {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub source: Option<GroundingChunkSource>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum GroundingChunkSource {
    Web {
        web: WebChunk,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    Image {
        image: ImageChunk,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    RetrievedContext {
        #[serde(rename = "retrievedContext")]
        retrieved_context: RetrievedContext,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    Maps {
        maps: MapsChunk,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RetrievedContext {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_metadata: Vec<CustomMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_search_store: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_id: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub value: Option<CustomMetadataValue>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum CustomMetadataValue {
    StringValue {
        #[serde(rename = "stringValue")]
        string_value: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    StringListValue {
        #[serde(rename = "stringListValue")]
        string_list_value: StringList,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    NumericValue {
        #[serde(rename = "numericValue")]
        numeric_value: f64,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    Raw(serde_json::Value),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, Default, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct StringList {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MapsChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_answer_sources: Option<PlaceAnswerSources>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct PlaceAnswerSources {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_snippets: Vec<ReviewSnippet>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ReviewSnippet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_maps_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GroundingSupport {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounding_chunk_indices: Vec<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confidence_scores: Vec<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rendered_parts: Vec<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<Segment>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Segment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RetrievalMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search_dynamic_retrieval_score: Option<f64>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UrlContextMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub url_metadata: Vec<UrlMetadata>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UrlMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieved_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_retrieval_status: Option<UrlRetrievalStatus>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
