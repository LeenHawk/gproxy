//! Request and non-streaming response bodies for `models.generateContent`.
//!
//! `model` is the HTTP path resource, not a JSON body field. Response fields
//! without a documented required constraint may be omitted by ProtoJSON;
//! null is treated as unset, and serialization omits unset fields.

use super::content::{Content, HarmCategory, SafetySetting, Tool, ToolConfig};
use super::count_tokens::ModalityTokenCount;
use super::generation::{GenerationConfig, ServiceTier};
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GenerateContentRequestBody {
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "tool_config")]
    pub tool_config: Option<ToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "safety_settings")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "system_instruction")]
    pub system_instruction: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "generation_config")]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "cached_content")]
    pub cached_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "service_tier")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type GenerateContentRequest = WireRequest<GenerateContentRequestBody>;
pub type GenerateContentResponse = WireResponse<GenerateContentResponseBody>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GenerateContentResponseBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<Candidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "prompt_feedback")]
    pub prompt_feedback: Option<PromptFeedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "usage_metadata")]
    pub usage_metadata: Option<UsageMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "model_version")]
    pub model_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_id")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "model_status")]
    pub model_status: Option<ModelStatus>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PromptFeedback {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "block_reason")]
    pub block_reason: Option<BlockReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "safety_ratings")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BlockReason {
    #[serde(rename = "BLOCK_REASON_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "SAFETY")]
    Safety,
    #[serde(rename = "OTHER")]
    Other,
    #[serde(rename = "BLOCKLIST")]
    Blocklist,
    #[serde(rename = "PROHIBITED_CONTENT")]
    ProhibitedContent,
    #[serde(rename = "IMAGE_SAFETY")]
    ImageSafety,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct UsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "prompt_token_count")]
    pub prompt_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "cached_content_token_count")]
    pub cached_content_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "candidates_token_count")]
    pub candidates_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "tool_use_prompt_token_count")]
    pub tool_use_prompt_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thoughts_token_count")]
    pub thoughts_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "total_token_count")]
    pub total_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "prompt_tokens_details")]
    pub prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "cache_tokens_details")]
    pub cache_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "candidates_tokens_details")]
    pub candidates_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "tool_use_prompt_tokens_details")]
    pub tool_use_prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "service_tier")]
    pub service_tier: Option<ServiceTier>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ModelStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "model_stage")]
    pub model_stage: Option<ModelStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "retirement_time")]
    pub retirement_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ModelStage {
    #[serde(rename = "MODEL_STAGE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "UNSTABLE_EXPERIMENTAL")]
    UnstableExperimental,
    #[serde(rename = "EXPERIMENTAL")]
    Experimental,
    #[serde(rename = "PREVIEW")]
    Preview,
    #[serde(rename = "STABLE")]
    Stable,
    #[serde(rename = "LEGACY")]
    Legacy,
    #[serde(rename = "DEPRECATED")]
    Deprecated,
    #[serde(rename = "RETIRED")]
    Retired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Candidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "finish_reason")]
    pub finish_reason: Option<FinishReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "safety_ratings")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "citation_metadata")]
    pub citation_metadata: Option<CitationMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "token_count")]
    pub token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "grounding_attributions")]
    pub grounding_attributions: Option<Vec<GroundingAttribution>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "grounding_metadata")]
    pub grounding_metadata: Option<GroundingMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "avg_logprobs")]
    pub avg_logprobs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "logprobs_result")]
    pub logprobs_result: Option<LogprobsResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "url_context_metadata")]
    pub url_context_metadata: Option<UrlContextMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "finish_message")]
    pub finish_message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FinishReason {
    #[serde(rename = "FINISH_REASON_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "STOP")]
    Stop,
    #[serde(rename = "MAX_TOKENS")]
    MaxTokens,
    #[serde(rename = "SAFETY")]
    Safety,
    #[serde(rename = "RECITATION")]
    Recitation,
    #[serde(rename = "LANGUAGE")]
    Language,
    #[serde(rename = "OTHER")]
    Other,
    #[serde(rename = "BLOCKLIST")]
    Blocklist,
    #[serde(rename = "PROHIBITED_CONTENT")]
    ProhibitedContent,
    #[serde(rename = "SPII")]
    Spii,
    #[serde(rename = "MALFORMED_FUNCTION_CALL")]
    MalformedFunctionCall,
    #[serde(rename = "IMAGE_SAFETY")]
    ImageSafety,
    #[serde(rename = "IMAGE_PROHIBITED_CONTENT")]
    ImageProhibitedContent,
    #[serde(rename = "IMAGE_OTHER")]
    ImageOther,
    #[serde(rename = "NO_IMAGE")]
    NoImage,
    #[serde(rename = "IMAGE_RECITATION")]
    ImageRecitation,
    #[serde(rename = "UNEXPECTED_TOOL_CALL")]
    UnexpectedToolCall,
    #[serde(rename = "TOO_MANY_TOOL_CALLS")]
    TooManyToolCalls,
    #[serde(rename = "MISSING_THOUGHT_SIGNATURE")]
    MissingThoughtSignature,
    #[serde(rename = "MALFORMED_RESPONSE")]
    MalformedResponse,
    #[serde(rename = "ESCALATION")]
    Escalation,
    #[serde(rename = "PUP_LIMITED_DISABLED")]
    PupLimitedDisabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum HarmProbability {
    #[serde(rename = "HARM_PROBABILITY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "NEGLIGIBLE")]
    Negligible,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GroundingAttribution {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "source_id")]
    pub source_id: Option<AttributionSourceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AttributionSourceId {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "grounding_passage")]
    pub grounding_passage: Option<GroundingPassageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "semantic_retriever_chunk")]
    pub semantic_retriever_chunk: Option<SemanticRetrieverChunk>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GroundingPassageId {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "passage_id")]
    pub passage_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "part_index")]
    pub part_index: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SemanticRetrieverChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GroundingMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "grounding_chunks")]
    pub grounding_chunks: Option<Vec<GroundingChunk>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "grounding_supports")]
    pub grounding_supports: Option<Vec<GroundingSupport>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "web_search_queries")]
    pub web_search_queries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "image_search_queries")]
    pub image_search_queries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "search_entry_point")]
    pub search_entry_point: Option<SearchEntryPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "retrieval_metadata")]
    pub retrieval_metadata: Option<RetrievalMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "google_maps_widget_context_token")]
    pub google_maps_widget_context_token: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SearchEntryPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "rendered_content")]
    pub rendered_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "sdk_blob")]
    pub sdk_blob: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GroundingChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<Web>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "retrieved_context")]
    pub retrieved_context: Option<RetrievedContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maps: Option<Maps>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Web {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Image {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "source_uri")]
    pub source_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "image_uri")]
    pub image_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct RetrievedContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "custom_metadata")]
    pub custom_metadata: Option<Vec<CustomMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "file_search_store")]
    pub file_search_store: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "page_number")]
    pub page_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "media_id")]
    pub media_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CustomMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "string_value")]
    pub string_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "string_list_value")]
    pub string_list_value: Option<StringList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "numeric_value")]
    pub numeric_value: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct StringList {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Maps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "place_id")]
    pub place_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "place_answer_sources")]
    pub place_answer_sources: Option<PlaceAnswerSources>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PlaceAnswerSources {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "review_snippets")]
    pub review_snippets: Option<Vec<ReviewSnippet>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ReviewSnippet {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "review_id")]
    pub review_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "google_maps_uri")]
    pub google_maps_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GroundingSupport {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "grounding_chunk_indices")]
    pub grounding_chunk_indices: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "confidence_scores")]
    pub confidence_scores: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "rendered_parts")]
    pub rendered_parts: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<Segment>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Segment {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "part_index")]
    pub part_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "start_index")]
    pub start_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "end_index")]
    pub end_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct RetrievalMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "google_search_dynamic_retrieval_score")]
    pub google_search_dynamic_retrieval_score: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LogprobsResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "top_candidates")]
    pub top_candidates: Option<Vec<TopCandidates>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "chosen_candidates")]
    pub chosen_candidates: Option<Vec<LogprobCandidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "log_probability_sum")]
    pub log_probability_sum: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TopCandidates {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<LogprobCandidate>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LogprobCandidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "token_id")]
    pub token_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "log_probability")]
    pub log_probability: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct UrlContextMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "url_metadata")]
    pub url_metadata: Option<Vec<UrlMetadata>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct UrlMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "retrieved_url")]
    pub retrieved_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "url_retrieval_status")]
    pub url_retrieval_status: Option<UrlRetrievalStatus>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum UrlRetrievalStatus {
    #[serde(rename = "URL_RETRIEVAL_STATUS_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "URL_RETRIEVAL_STATUS_SUCCESS")]
    Success,
    #[serde(rename = "URL_RETRIEVAL_STATUS_ERROR")]
    Error,
    #[serde(rename = "URL_RETRIEVAL_STATUS_PAYWALL")]
    Paywall,
    #[serde(rename = "URL_RETRIEVAL_STATUS_UNSAFE")]
    Unsafe,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CitationMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "citation_sources")]
    pub citation_sources: Option<Vec<CitationSource>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CitationSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "start_index")]
    pub start_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "end_index")]
    pub end_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
