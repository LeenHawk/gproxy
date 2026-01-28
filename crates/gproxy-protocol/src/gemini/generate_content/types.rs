use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::gemini::count_tokens::types::{Content, JsonValue, Modality, ModalityTokenCount};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Type {
    #[serde(rename = "TYPE_UNSPECIFIED")]
    TypeUnspecified,
    #[serde(rename = "STRING")]
    String,
    #[serde(rename = "NUMBER")]
    Number,
    #[serde(rename = "INTEGER")]
    Integer,
    #[serde(rename = "BOOLEAN")]
    Boolean,
    #[serde(rename = "ARRAY")]
    Array,
    #[serde(rename = "OBJECT")]
    Object,
    #[serde(rename = "NULL")]
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SchemaOrBool {
    Bool(bool),
    Schema(Box<Schema>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(rename = "type")]
    pub r#type: Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    /// Int64 is encoded as string in the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<String>,
    /// Int64 is encoded as string in the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    /// Int64 is encoded as string in the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<String>,
    /// Int64 is encoded as string in the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<String>,
    /// Int64 is encoded as string in the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<String>,
    /// Int64 is encoded as string in the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<JsonValue>,
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_items: Option<Vec<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_ordering: Option<Vec<String>>,
    /// Minimum value of Type.INTEGER and Type.NUMBER.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    /// Maximum value of Type.INTEGER and Type.NUMBER.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    /// additionalProperties can be a boolean or a Schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<SchemaOrBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Behavior {
    #[serde(rename = "UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "BLOCKING")]
    Blocking,
    #[serde(rename = "NON_BLOCKING")]
    NonBlocking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<Behavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Schema>,
    /// Mutually exclusive with parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters_json_schema: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Schema>,
    /// Mutually exclusive with response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_declarations: Option<Vec<FunctionDeclaration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search_retrieval: Option<GoogleSearchRetrieval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_execution: Option<CodeExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search: Option<GoogleSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<ComputerUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_context: Option<UrlContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_search: Option<FileSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_maps: Option<GoogleMaps>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSearchRetrieval {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_retrieval_config: Option<DynamicRetrievalConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DynamicRetrievalMode {
    #[serde(rename = "MODE_UNSPECIFIED")]
    ModeUnspecified,
    #[serde(rename = "MODE_DYNAMIC")]
    ModeDynamic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicRetrievalConfig {
    pub mode: DynamicRetrievalMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeExecution {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlContext {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSearch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range_filter: Option<Interval>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interval {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Environment {
    #[serde(rename = "ENVIRONMENT_UNSPECIFIED")]
    EnvironmentUnspecified,
    #[serde(rename = "ENVIRONMENT_BROWSER")]
    EnvironmentBrowser,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUse {
    pub environment: Environment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_predefined_functions: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearch {
    pub file_search_store_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleMaps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_widget: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_calling_config: Option<FunctionCallingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_config: Option<RetrievalConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionCallingMode {
    #[serde(rename = "MODE_UNSPECIFIED")]
    ModeUnspecified,
    #[serde(rename = "AUTO")]
    Auto,
    #[serde(rename = "ANY")]
    Any,
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "VALIDATED")]
    Validated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<FunctionCallingMode>,
    /// Only applicable when mode is ANY or VALIDATED.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat_lng: Option<LatLng>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatLng {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HarmCategory {
    #[serde(rename = "HARM_CATEGORY_UNSPECIFIED")]
    HarmCategoryUnspecified,
    #[serde(rename = "HARM_CATEGORY_DEROGATORY")]
    HarmCategoryDerogatory,
    #[serde(rename = "HARM_CATEGORY_TOXICITY")]
    HarmCategoryToxicity,
    #[serde(rename = "HARM_CATEGORY_VIOLENCE")]
    HarmCategoryViolence,
    #[serde(rename = "HARM_CATEGORY_SEXUAL")]
    HarmCategorySexual,
    #[serde(rename = "HARM_CATEGORY_MEDICAL")]
    HarmCategoryMedical,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS")]
    HarmCategoryDangerous,
    #[serde(rename = "HARM_CATEGORY_HARASSMENT")]
    HarmCategoryHarassment,
    #[serde(rename = "HARM_CATEGORY_HATE_SPEECH")]
    HarmCategoryHateSpeech,
    #[serde(rename = "HARM_CATEGORY_SEXUALLY_EXPLICIT")]
    HarmCategorySexuallyExplicit,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS_CONTENT")]
    HarmCategoryDangerousContent,
    #[serde(rename = "HARM_CATEGORY_CIVIC_INTEGRITY")]
    HarmCategoryCivicIntegrity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HarmBlockThreshold {
    #[serde(rename = "HARM_BLOCK_THRESHOLD_UNSPECIFIED")]
    HarmBlockThresholdUnspecified,
    #[serde(rename = "BLOCK_LOW_AND_ABOVE")]
    BlockLowAndAbove,
    #[serde(rename = "BLOCK_MEDIUM_AND_ABOVE")]
    BlockMediumAndAbove,
    #[serde(rename = "BLOCK_ONLY_HIGH")]
    BlockOnlyHigh,
    #[serde(rename = "BLOCK_NONE")]
    BlockNone,
    #[serde(rename = "OFF")]
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    pub category: HarmCategory,
    pub threshold: HarmBlockThreshold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HarmProbability {
    #[serde(rename = "HARM_PROBABILITY_UNSPECIFIED")]
    HarmProbabilityUnspecified,
    #[serde(rename = "NEGLIGIBLE")]
    Negligible,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockReason {
    #[serde(rename = "BLOCK_REASON_UNSPECIFIED")]
    BlockReasonUnspecified,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<BlockReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelStage {
    #[serde(rename = "MODEL_STAGE_UNSPECIFIED")]
    ModelStageUnspecified,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub model_stage: ModelStage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retirement_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FinishReason {
    #[serde(rename = "FINISH_REASON_UNSPECIFIED")]
    FinishReasonUnspecified,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationMetadata {
    pub citation_sources: Vec<CitationSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributionSourceId {
    #[serde(rename_all = "camelCase")]
    GroundingPassage {
        grounding_passage: GroundingPassageId,
    },
    #[serde(rename_all = "camelCase")]
    SemanticRetrieverChunk {
        semantic_retriever_chunk: SemanticRetrieverChunk,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingPassageId {
    pub passage_id: String,
    pub part_index: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRetrieverChunk {
    pub source: String,
    pub chunk: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingAttribution {
    pub source_id: AttributionSourceId,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GroundingChunk {
    #[serde(rename_all = "camelCase")]
    Web { web: Web },
    #[serde(rename_all = "camelCase")]
    RetrievedContext { retrieved_context: RetrievedContext },
    #[serde(rename_all = "camelCase")]
    Maps { maps: Maps },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Web {
    pub uri: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievedContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_search_store: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Maps {
    pub uri: String,
    pub title: String,
    pub text: String,
    pub place_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_answer_sources: Option<PlaceAnswerSources>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceAnswerSources {
    pub review_snippets: Vec<ReviewSnippet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSnippet {
    pub review_id: String,
    pub google_maps_uri: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub part_index: u32,
    pub start_index: u32,
    pub end_index: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingSupport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunk_indices: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_scores: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment: Option<Segment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search_dynamic_retrieval_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntryPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_content: Option<String>,
    /// Base64-encoded JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingMetadata {
    pub grounding_chunks: Vec<GroundingChunk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_supports: Option<Vec<GroundingSupport>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_queries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_entry_point: Option<SearchEntryPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_metadata: Option<RetrievalMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_maps_widget_context_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogprobsCandidate {
    pub token: String,
    pub token_id: u32,
    pub log_probability: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopCandidates {
    pub candidates: Vec<LogprobsCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogprobsResult {
    pub top_candidates: Vec<TopCandidates>,
    pub chosen_candidates: Vec<LogprobsCandidate>,
    pub log_probability_sum: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UrlRetrievalStatus {
    #[serde(rename = "URL_RETRIEVAL_STATUS_UNSPECIFIED")]
    UrlRetrievalStatusUnspecified,
    #[serde(rename = "URL_RETRIEVAL_STATUS_SUCCESS")]
    UrlRetrievalStatusSuccess,
    #[serde(rename = "URL_RETRIEVAL_STATUS_ERROR")]
    UrlRetrievalStatusError,
    #[serde(rename = "URL_RETRIEVAL_STATUS_PAYWALL")]
    UrlRetrievalStatusPaywall,
    #[serde(rename = "URL_RETRIEVAL_STATUS_UNSAFE")]
    UrlRetrievalStatusUnsafe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlMetadata {
    pub retrieved_url: String,
    pub url_retrieval_status: UrlRetrievalStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlContextMetadata {
    pub url_metadata: Vec<UrlMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_metadata: Option<CitationMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_attributions: Option<Vec<GroundingAttribution>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_metadata: Option<GroundingMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_logprobs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs_result: Option<LogprobsResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_context_metadata: Option<UrlContextMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThinkingLevel {
    #[serde(rename = "THINKING_LEVEL_UNSPECIFIED")]
    ThinkingLevelUnspecified,
    #[serde(rename = "MINIMAL")]
    Minimal,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    pub include_thoughts: bool,
    pub thinking_budget: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_size: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaResolution {
    #[serde(rename = "MEDIA_RESOLUTION_UNSPECIFIED")]
    MediaResolutionUnspecified,
    #[serde(rename = "MEDIA_RESOLUTION_LOW")]
    MediaResolutionLow,
    #[serde(rename = "MEDIA_RESOLUTION_MEDIUM")]
    MediaResolutionMedium,
    #[serde(rename = "MEDIA_RESOLUTION_HIGH")]
    MediaResolutionHigh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrebuiltVoiceConfig {
    pub voice_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
    /// Mutually exclusive with multi_speaker_voice_config in SpeechConfig.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebuilt_voice_config: Option<PrebuiltVoiceConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerVoiceConfig {
    pub speaker: String,
    pub voice_config: VoiceConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSpeakerVoiceConfig {
    pub speaker_voice_configs: Vec<SpeakerVoiceConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_config: Option<VoiceConfig>,
    /// Mutually exclusive with voice_config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_speaker_voice_config: Option<MultiSpeakerVoiceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    /// Mutually exclusive with response_json_schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Schema>,
    /// Mutually exclusive with response_schema. JSON Schema.
    #[serde(
        rename = "_responseJsonSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_json_schema_internal: Option<JsonValue>,
    /// JSON Schema (preferred).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_modalities: Option<Vec<Modality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_logprobs: Option<bool>,
    /// Valid range: 0..=20.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_enhanced_civic_answers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<SpeechConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<ImageConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_resolution: Option<MediaResolution>,
}
