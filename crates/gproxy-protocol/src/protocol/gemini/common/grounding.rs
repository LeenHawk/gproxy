use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ExtraFields, UrlRetrievalStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntryPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_blob: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroundingChunk {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub source: Option<GroundingChunkSource>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GroundingChunkSource {
    Web {
        web: WebChunk,
    },
    Image {
        image: ImageChunk,
    },
    RetrievedContext {
        #[serde(rename = "retrievedContext")]
        retrieved_context: RetrievedContext,
    },
    Maps {
        maps: MapsChunk,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebChunk {
    pub uri: Option<String>,
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageChunk {
    pub source_uri: Option<String>,
    pub image_uri: Option<String>,
    pub title: Option<String>,
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetrievedContext {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_metadata: Vec<CustomMetadata>,
    pub uri: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub file_search_store: Option<String>,
    pub page_number: Option<i32>,
    pub media_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub value: Option<CustomMetadataValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomMetadataValue {
    StringValue {
        #[serde(rename = "stringValue")]
        string_value: String,
    },
    StringListValue {
        #[serde(rename = "stringListValue")]
        string_list_value: StringList,
    },
    NumericValue {
        #[serde(rename = "numericValue")]
        numeric_value: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StringList {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MapsChunk {
    pub uri: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub place_id: Option<String>,
    pub place_answer_sources: Option<PlaceAnswerSources>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlaceAnswerSources {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_snippets: Vec<ReviewSnippet>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSnippet {
    pub review_id: Option<String>,
    pub google_maps_uri: Option<String>,
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroundingSupport {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounding_chunk_indices: Vec<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confidence_scores: Vec<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rendered_parts: Vec<i32>,
    pub segment: Option<Segment>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub part_index: Option<i32>,
    pub start_index: Option<i32>,
    pub end_index: Option<i32>,
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalMetadata {
    pub google_search_dynamic_retrieval_score: Option<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UrlContextMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub url_metadata: Vec<UrlMetadata>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UrlMetadata {
    pub retrieved_url: Option<String>,
    pub url_retrieval_status: Option<UrlRetrievalStatus>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}
