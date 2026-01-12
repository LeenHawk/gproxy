use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;
use std::collections::{HashMap, HashSet};

pub type JsonStruct = Map<String, Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentRole {
    User,
    Model,
}

fn default_content_role() -> Option<ContentRole> {
    Some(ContentRole::User)
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    #[serde(default)]
    #[validate(min_items = 1)]
    #[validate]
    pub parts: Vec<Part>,
    #[serde(
        default = "default_content_role",
        skip_serializing_if = "Option::is_none"
    )]
    pub role: Option<ContentRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct SystemInstruction {
    #[validate(min_items = 1)]
    pub parts: Vec<TextPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPart {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_part)]
pub struct Part {
    #[serde(flatten)]
    #[validate]
    pub data: PartData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_metadata: Option<JsonStruct>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub video_metadata: Option<VideoMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase", untagged)]
pub enum PartData {
    Text {
        text: String,
    },
    InlineData {
        #[serde(alias = "inline_data")]
        #[validate]
        inline_data: Blob,
    },
    FunctionCall {
        #[serde(alias = "function_call")]
        #[validate]
        function_call: FunctionCall,
    },
    FunctionResponse {
        #[serde(alias = "function_response")]
        #[validate]
        function_response: FunctionResponse,
    },
    FileData {
        #[serde(alias = "file_data")]
        #[validate]
        file_data: FileData,
    },
    ExecutableCode {
        #[serde(alias = "executable_code")]
        executable_code: ExecutableCode,
    },
    CodeExecutionResult {
        #[serde(alias = "code_execution_result")]
        code_execution_result: CodeExecutionResult,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    #[serde(alias = "mime_type")]
    #[validate(custom = validate_inline_mime_type)]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_file_data)]
pub struct FileData {
    #[serde(skip_serializing_if = "Option::is_none", alias = "mime_type")]
    pub mime_type: Option<String>,
    #[serde(alias = "file_uri")]
    pub file_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(max_length = 64)]
    #[validate(custom = |name| validate_function_name_chars(name, false))]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<JsonStruct>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[validate(max_length = 64)]
    #[validate(custom = |name| validate_function_name_chars(name, false))]
    pub name: String,
    pub response: JsonStruct,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub parts: Option<Vec<FunctionResponsePart>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub will_continue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling: Option<Scheduling>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponsePart {
    #[serde(alias = "inline_data")]
    #[validate]
    pub inline_data: FunctionResponseBlob,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponseBlob {
    #[serde(alias = "mime_type")]
    #[validate(custom = validate_inline_mime_type)]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Scheduling {
    SchedulingUnspecified,
    Silent,
    WhenIdle,
    Interrupt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Language {
    LanguageUnspecified,
    Python,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableCode {
    pub language: Language,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    OutcomeUnspecified,
    OutcomeOk,
    OutcomeFailed,
    OutcomeDeadlineExceeded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeExecutionResult {
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(exclusive_minimum = 0.0)]
    #[validate(maximum = 24.0)]
    pub fps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub function_declarations: Option<Vec<FunctionDeclaration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search_retrieval: Option<GoogleSearchRetrieval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_execution: Option<CodeExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub google_search: Option<GoogleSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<ComputerUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_context: Option<UrlContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub file_search: Option<FileSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_maps: Option<GoogleMaps>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_function_declaration)]
pub struct FunctionDeclaration {
    #[validate(max_length = 64)]
    #[validate(custom = |name| validate_function_name_chars(name, true))]
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<Behavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters_json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Behavior {
    Unspecified,
    Blocking,
    NonBlocking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(rename = "type")]
    pub schema_type: SchemaType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#enum: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_ordering: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchemaType {
    TypeUnspecified,
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSearchRetrieval {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_retrieval_config: Option<DynamicRetrievalConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicRetrievalConfig {
    pub mode: DynamicRetrievalMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_threshold: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DynamicRetrievalMode {
    ModeUnspecified,
    ModeDynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecution {}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSearch {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub time_range_filter: Option<Interval>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_interval)]
pub struct Interval {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUse {
    pub environment: Environment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_predefined_functions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Environment {
    EnvironmentUnspecified,
    EnvironmentBrowser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlContext {}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct FileSearch {
    #[validate(min_items = 1)]
    pub file_search_store_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleMaps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_widget: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub function_calling_config: Option<FunctionCallingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub retrieval_config: Option<RetrievalConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_function_calling_config)]
pub struct FunctionCallingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<FunctionCallingMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FunctionCallingMode {
    ModeUnspecified,
    Auto,
    Any,
    None,
    Validated,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub lat_lng: Option<LatLng>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct LatLng {
    #[validate(minimum = -90.0)]
    #[validate(maximum = 90.0)]
    pub latitude: f64,
    #[validate(minimum = -180.0)]
    #[validate(maximum = 180.0)]
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_generation_config)]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none", alias = "stop_sequences")]
    #[validate(max_items = 5)]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "response_mime_type")]
    pub response_mime_type: Option<ResponseMimeType>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "response_schema")]
    pub response_schema: Option<Schema>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "_responseJsonSchema",
        alias = "_response_json_schema"
    )]
    pub response_json_schema_internal: Option<Value>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "response_json_schema"
    )]
    pub response_json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "response_modalities")]
    pub response_modalities: Option<Vec<ResponseModality>>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "candidate_count")]
    pub candidate_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "max_output_tokens")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0.0)]
    #[validate(maximum = 2.0)]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "top_p")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "top_k")]
    pub top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "presence_penalty")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "frequency_penalty")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "response_logprobs")]
    pub response_logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0)]
    #[validate(maximum = 20)]
    pub logprobs: Option<i64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "enable_enhanced_civic_answers"
    )]
    pub enable_enhanced_civic_answers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "speech_config")]
    #[validate]
    pub speech_config: Option<SpeechConfig>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "thinking_config")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "image_config")]
    pub image_config: Option<ImageConfig>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "media_resolution")]
    pub media_resolution: Option<MediaResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResponseMimeType {
    #[serde(rename = "text/plain")]
    TextPlain,
    #[serde(rename = "application/json")]
    ApplicationJson,
    #[serde(rename = "text/x.enum")]
    TextXEnum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResponseModality {
    ModalityUnspecified,
    Text,
    Image,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_speech_config)]
pub struct SpeechConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub voice_config: Option<VoiceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub multi_speaker_voice_config: Option<MultiSpeakerVoiceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<SpeechLanguageCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpeechLanguageCode {
    #[serde(rename = "de-DE")]
    DeDe,
    #[serde(rename = "en-AU")]
    EnAu,
    #[serde(rename = "en-GB")]
    EnGb,
    #[serde(rename = "en-IN")]
    EnIn,
    #[serde(rename = "en-US")]
    EnUs,
    #[serde(rename = "es-US")]
    EsUs,
    #[serde(rename = "fr-FR")]
    FrFr,
    #[serde(rename = "hi-IN")]
    HiIn,
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "ar-XA")]
    ArXa,
    #[serde(rename = "es-ES")]
    EsEs,
    #[serde(rename = "fr-CA")]
    FrCa,
    #[serde(rename = "id-ID")]
    IdId,
    #[serde(rename = "it-IT")]
    ItIt,
    #[serde(rename = "ja-JP")]
    JaJp,
    #[serde(rename = "tr-TR")]
    TrTr,
    #[serde(rename = "vi-VN")]
    ViVn,
    #[serde(rename = "bn-IN")]
    BnIn,
    #[serde(rename = "gu-IN")]
    GuIn,
    #[serde(rename = "kn-IN")]
    KnIn,
    #[serde(rename = "ml-IN")]
    MlIn,
    #[serde(rename = "mr-IN")]
    MrIn,
    #[serde(rename = "ta-IN")]
    TaIn,
    #[serde(rename = "te-IN")]
    TeIn,
    #[serde(rename = "nl-NL")]
    NlNl,
    #[serde(rename = "ko-KR")]
    KoKr,
    #[serde(rename = "cmn-CN")]
    CmnCn,
    #[serde(rename = "pl-PL")]
    PlPl,
    #[serde(rename = "ru-RU")]
    RuRu,
    #[serde(rename = "th-TH")]
    ThTh,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_voice_config)]
pub struct VoiceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebuilt_voice_config: Option<PrebuiltVoiceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrebuiltVoiceConfig {
    pub voice_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct MultiSpeakerVoiceConfig {
    #[validate(min_items = 1)]
    #[validate]
    pub speaker_voice_configs: Vec<SpeakerVoiceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerVoiceConfig {
    pub speaker: String,
    #[validate]
    pub voice_config: VoiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ThinkingLevel {
    ThinkingLevelUnspecified,
    Low,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<AspectRatio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_size: Option<ImageSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AspectRatio {
    #[serde(rename = "1:1")]
    Ratio1x1,
    #[serde(rename = "2:3")]
    Ratio2x3,
    #[serde(rename = "3:2")]
    Ratio3x2,
    #[serde(rename = "3:4")]
    Ratio3x4,
    #[serde(rename = "4:3")]
    Ratio4x3,
    #[serde(rename = "9:16")]
    Ratio9x16,
    #[serde(rename = "16:9")]
    Ratio16x9,
    #[serde(rename = "21:9")]
    Ratio21x9,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSize {
    #[serde(rename = "1K")]
    Size1k,
    #[serde(rename = "2K")]
    Size2k,
    #[serde(rename = "4K")]
    Size4k,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MediaResolution {
    MediaResolutionUnspecified,
    MediaResolutionLow,
    MediaResolutionMedium,
    MediaResolutionHigh,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HarmCategory {
    HarmCategoryUnspecified,
    HarmCategoryDerogatory,
    HarmCategoryToxicity,
    HarmCategoryViolence,
    HarmCategorySexual,
    HarmCategoryMedical,
    HarmCategoryDangerous,
    HarmCategoryHarassment,
    HarmCategoryHateSpeech,
    HarmCategorySexuallyExplicit,
    HarmCategoryDangerousContent,
    HarmCategoryCivicIntegrity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HarmProbability {
    HarmProbabilityUnspecified,
    Negligible,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HarmBlockThreshold {
    HarmBlockThresholdUnspecified,
    BlockLowAndAbove,
    BlockMediumAndAbove,
    BlockOnlyHigh,
    BlockNone,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    pub category: HarmCategory,
    pub threshold: HarmBlockThreshold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlockReason {
    BlockReasonUnspecified,
    Safety,
    Other,
    Blocklist,
    ProhibitedContent,
    ImageSafety,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<BlockReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinishReason {
    FinishReasonUnspecified,
    Stop,
    MaxTokens,
    Safety,
    Recitation,
    Language,
    Other,
    Blocklist,
    ProhibitedContent,
    Spii,
    MalformedFunctionCall,
    ImageSafety,
    ImageProhibitedContent,
    ImageOther,
    NoImage,
    ImageRecitation,
    UnexpectedToolCall,
    TooManyToolCalls,
    MissingThoughtSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationMetadata {
    pub citation_sources: Vec<CitationSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingAttribution {
    pub source_id: AttributionSourceId,
    pub content: Content,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionSourceId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_passage: Option<GroundingPassageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_retriever_chunk: Option<SemanticRetrieverChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingPassageId {
    pub passage_id: String,
    pub part_index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRetrieverChunk {
    pub source: String,
    pub chunk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunks: Option<Vec<GroundingChunk>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntryPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_blob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<WebChunk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieved_context: Option<RetrievedContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maps: Option<MapsChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebChunk {
    pub uri: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapsChunk {
    pub uri: String,
    pub title: String,
    pub text: String,
    pub place_id: String,
    pub place_answer_sources: PlaceAnswerSources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceAnswerSources {
    pub review_snippets: Vec<ReviewSnippet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSnippet {
    pub review_id: String,
    pub google_maps_uri: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingSupport {
    pub grounding_chunk_indices: Vec<i64>,
    pub confidence_scores: Vec<f64>,
    pub segment: Segment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub part_index: i64,
    pub start_index: i64,
    pub end_index: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalMetadata {
    pub google_search_dynamic_retrieval_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogprobsResult {
    pub top_candidates: Vec<TopCandidates>,
    pub chosen_candidates: Vec<LogprobCandidate>,
    pub log_probability_sum: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopCandidates {
    pub candidates: Vec<LogprobCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogprobCandidate {
    pub token: String,
    pub token_id: i64,
    pub log_probability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlContextMetadata {
    pub url_metadata: Vec<UrlMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlMetadata {
    pub retrieved_url: String,
    pub url_retrieval_status: UrlRetrievalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UrlRetrievalStatus {
    UrlRetrievalStatusUnspecified,
    UrlRetrievalStatusSuccess,
    UrlRetrievalStatusError,
    UrlRetrievalStatusPaywall,
    UrlRetrievalStatusUnsafe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_metadata: Option<CitationMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<i64>,
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
    pub index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModalityTokenCount {
    pub modality: ModalityToken,
    pub token_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModalityToken {
    ModalityUnspecified,
    Text,
    Image,
    Video,
    Audio,
    Document,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model_id: Option<String>,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_token_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_generation_methods: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
}

fn custom_error(message: impl Into<String>) -> ValidationError {
    ValidationError::Custom(message.into())
}

fn validate_part(part: &Part) -> Result<(), ValidationError> {
    if part.video_metadata.is_some() {
        let has_media = matches!(
            part.data,
            PartData::InlineData { .. } | PartData::FileData { .. }
        );
        if !has_media {
            return Err(custom_error(
                "videoMetadata requires inlineData or fileData",
            ));
        }
    }
    Ok(())
}

fn validate_function_name_chars(name: &str, allow_colon_dot: bool) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(custom_error("function name must not be empty"));
    }
    let valid = name.chars().all(|ch| {
        ch.is_ascii_alphanumeric()
            || ch == '_'
            || ch == '-'
            || (allow_colon_dot && (ch == ':' || ch == '.'))
    });
    if !valid {
        return Err(custom_error("function name contains invalid characters"));
    }
    Ok(())
}

fn validate_inline_mime_type(mime_type: &str) -> Result<(), ValidationError> {
    if is_supported_inline_mime_type(mime_type) {
        Ok(())
    } else {
        Err(custom_error(format!("unsupported mime_type: {mime_type}")))
    }
}

fn validate_file_mime_type(mime_type: &str) -> Result<(), ValidationError> {
    if is_supported_file_mime_type(mime_type) {
        Ok(())
    } else {
        Err(custom_error(format!("unsupported mime_type: {mime_type}")))
    }
}

fn validate_file_data(file_data: &FileData) -> Result<(), ValidationError> {
    if file_data.file_uri.trim().is_empty() {
        return Err(custom_error("fileData.fileUri must not be empty"));
    }
    if let Some(mime_type) = &file_data.mime_type {
        validate_file_mime_type(mime_type)?;
    }
    Ok(())
}

fn validate_function_declaration(declaration: &FunctionDeclaration) -> Result<(), ValidationError> {
    if declaration.parameters.is_some() && declaration.parameters_json_schema.is_some() {
        return Err(custom_error(
            "parameters and parametersJsonSchema are mutually exclusive",
        ));
    }
    if declaration.response.is_some() && declaration.response_json_schema.is_some() {
        return Err(custom_error(
            "response and responseJsonSchema are mutually exclusive",
        ));
    }
    if let Some(schema) = &declaration.parameters_json_schema
        && !schema.is_object()
    {
        return Err(custom_error("parametersJsonSchema must be a JSON object"));
    }
    if let Some(schema) = &declaration.response_json_schema
        && !schema.is_object()
    {
        return Err(custom_error("responseJsonSchema must be a JSON object"));
    }
    Ok(())
}

fn validate_interval(interval: &Interval) -> Result<(), ValidationError> {
    if interval.start_time.is_some() != interval.end_time.is_some() {
        return Err(custom_error(
            "googleSearch.timeRangeFilter requires both startTime and endTime",
        ));
    }
    Ok(())
}

fn validate_function_calling_config(config: &FunctionCallingConfig) -> Result<(), ValidationError> {
    let Some(allowed) = &config.allowed_function_names else {
        return Ok(());
    };

    let valid_mode = matches!(
        config.mode,
        Some(FunctionCallingMode::Any) | Some(FunctionCallingMode::Validated)
    );
    if !valid_mode {
        return Err(custom_error(
            "allowedFunctionNames requires mode ANY or VALIDATED",
        ));
    }
    if allowed.is_empty() {
        return Err(custom_error("allowedFunctionNames must not be empty"));
    }

    let mut seen = HashSet::new();
    for name in allowed {
        if name.chars().count() > 64 {
            return Err(custom_error(
                "allowedFunctionNames must be at most 64 characters",
            ));
        }
        if !seen.insert(name.as_str()) {
            return Err(custom_error("allowedFunctionNames must be unique"));
        }
        validate_function_name_chars(name, true)?;
    }

    Ok(())
}

fn validate_generation_config(config: &GenerationConfig) -> Result<(), ValidationError> {
    if config.logprobs.is_some() && config.response_logprobs != Some(true) {
        return Err(custom_error("logprobs requires responseLogprobs=true"));
    }

    let has_json_schema =
        config.response_json_schema_internal.is_some() || config.response_json_schema.is_some();

    if config.response_schema.is_some() && has_json_schema {
        return Err(custom_error(
            "responseSchema is mutually exclusive with responseJsonSchema",
        ));
    }

    if config.response_json_schema_internal.is_some() && config.response_json_schema.is_some() {
        return Err(custom_error(
            "responseJsonSchema and _responseJsonSchema are mutually exclusive",
        ));
    }

    if (config.response_schema.is_some() || has_json_schema)
        && config.response_mime_type != Some(ResponseMimeType::ApplicationJson)
    {
        return Err(custom_error(
            "responseMimeType must be application/json when using a schema",
        ));
    }

    if let Some(schema) = &config.response_json_schema_internal {
        validate_json_schema(schema, "_responseJsonSchema")?;
    }

    if let Some(schema) = &config.response_json_schema {
        validate_json_schema(schema, "responseJsonSchema")?;
    }

    Ok(())
}

fn validate_speech_config(config: &SpeechConfig) -> Result<(), ValidationError> {
    if config.voice_config.is_some() && config.multi_speaker_voice_config.is_some() {
        return Err(custom_error(
            "voiceConfig and multiSpeakerVoiceConfig are mutually exclusive",
        ));
    }
    Ok(())
}

fn validate_voice_config(config: &VoiceConfig) -> Result<(), ValidationError> {
    if config.prebuilt_voice_config.is_none() {
        return Err(custom_error("voiceConfig requires prebuiltVoiceConfig"));
    }
    Ok(())
}

fn validate_json_schema(schema: &Value, field: &str) -> Result<(), ValidationError> {
    let obj = schema
        .as_object()
        .ok_or_else(|| custom_error(format!("{field} must be a JSON object")))?;
    validate_schema_object(obj, field)
}

fn validate_schema_object(obj: &Map<String, Value>, path: &str) -> Result<(), ValidationError> {
    if obj.contains_key("$ref") {
        for key in obj.keys() {
            if !key.starts_with('$') {
                return Err(custom_error(format!(
                    "{path} with $ref cannot include {key}"
                )));
            }
        }
    }

    for (key, value) in obj {
        match key.as_str() {
            "$id" | "$anchor" | "$ref" => {
                if !value.is_string() {
                    return Err(custom_error(format!("{path}.{key} must be a string")));
                }
            }
            "$defs" => {
                let defs = value
                    .as_object()
                    .ok_or_else(|| custom_error(format!("{path}.{key} must be an object")))?;
                for (def_key, def_val) in defs {
                    let def_path = format!("{path}.{key}.{def_key}");
                    let def_obj = def_val
                        .as_object()
                        .ok_or_else(|| custom_error(format!("{def_path} must be an object")))?;
                    validate_schema_object(def_obj, &def_path)?;
                }
            }
            "type" => {
                if let Some(type_str) = value.as_str() {
                    if type_str.is_empty() {
                        return Err(custom_error(format!("{path}.{key} must not be empty")));
                    }
                } else if let Some(types) = value.as_array() {
                    if types.is_empty() {
                        return Err(custom_error(format!("{path}.{key} must not be empty")));
                    }
                    for entry in types {
                        if !entry.is_string() {
                            return Err(custom_error(format!(
                                "{path}.{key} items must be strings"
                            )));
                        }
                    }
                } else {
                    return Err(custom_error(format!(
                        "{path}.{key} must be a string or array of strings"
                    )));
                }
            }
            "format" | "title" | "description" => {
                if !value.is_string() {
                    return Err(custom_error(format!("{path}.{key} must be a string")));
                }
            }
            "enum" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| custom_error(format!("{path}.{key} must be an array")))?;
                for entry in items {
                    if !(entry.is_string() || entry.is_number()) {
                        return Err(custom_error(format!(
                            "{path}.{key} items must be strings or numbers"
                        )));
                    }
                }
            }
            "items" => {
                let item_obj = value
                    .as_object()
                    .ok_or_else(|| custom_error(format!("{path}.{key} must be an object")))?;
                validate_schema_object(item_obj, &format!("{path}.{key}"))?;
            }
            "prefixItems" | "anyOf" | "oneOf" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| custom_error(format!("{path}.{key} must be an array")))?;
                for (idx, entry) in items.iter().enumerate() {
                    let entry_obj = entry.as_object().ok_or_else(|| {
                        custom_error(format!("{path}.{key}[{idx}] must be an object"))
                    })?;
                    validate_schema_object(entry_obj, &format!("{path}.{key}[{idx}]"))?;
                }
            }
            "minItems" | "maxItems" => {
                if value.as_i64().is_none() {
                    return Err(custom_error(format!("{path}.{key} must be an integer")));
                }
            }
            "minimum" | "maximum" => {
                if !value.is_number() {
                    return Err(custom_error(format!("{path}.{key} must be a number")));
                }
            }
            "properties" => {
                let props = value
                    .as_object()
                    .ok_or_else(|| custom_error(format!("{path}.{key} must be an object")))?;
                for (prop_key, prop_val) in props {
                    let prop_path = format!("{path}.{key}.{prop_key}");
                    let prop_obj = prop_val
                        .as_object()
                        .ok_or_else(|| custom_error(format!("{prop_path} must be an object")))?;
                    validate_schema_object(prop_obj, &prop_path)?;
                }
            }
            "additionalProperties" => {
                if value.is_boolean() {
                    continue;
                }
                let prop_obj = value.as_object().ok_or_else(|| {
                    custom_error(format!("{path}.{key} must be a boolean or object"))
                })?;
                validate_schema_object(prop_obj, &format!("{path}.{key}"))?;
            }
            "required" | "propertyOrdering" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| custom_error(format!("{path}.{key} must be an array")))?;
                for entry in items {
                    if !entry.is_string() {
                        return Err(custom_error(format!("{path}.{key} items must be strings")));
                    }
                }
            }
            _ => {
                return Err(custom_error(format!(
                    "{path} contains unsupported schema key: {key}"
                )));
            }
        }
    }

    Ok(())
}

fn is_supported_inline_mime_type(mime_type: &str) -> bool {
    matches!(
        mime_type,
        // Image formats.
        "image/png"
            | "image/jpeg"
            | "image/webp"
            | "image/heic"
            | "image/heif"
            // Audio formats.
            | "audio/wav"
            | "audio/mp3"
            | "audio/mpeg"
            | "audio/aiff"
            | "audio/aac"
            | "audio/ogg"
            | "audio/flac"
            // Video formats.
            | "video/mp4"
            | "video/mpeg"
            | "video/mov"
            | "video/avi"
            | "video/x-flv"
            | "video/mpg"
            | "video/webm"
            | "video/wmv"
            | "video/3gpp"
            // Document formats.
            | "application/pdf"
    )
}

fn is_supported_file_mime_type(mime_type: &str) -> bool {
    matches!(
        mime_type,
        // Image formats.
        "image/png"
            | "image/jpeg"
            | "image/webp"
            | "image/heic"
            | "image/heif"
            // Audio formats.
            | "audio/wav"
            | "audio/mp3"
            | "audio/mpeg"
            | "audio/aiff"
            | "audio/aac"
            | "audio/ogg"
            | "audio/flac"
            // Video formats.
            | "video/mp4"
            | "video/mpeg"
            | "video/mov"
            | "video/avi"
            | "video/x-flv"
            | "video/mpg"
            | "video/webm"
            | "video/wmv"
            | "video/3gpp"
            // Plain text formats.
            | "text/plain"
            | "text/html"
            | "text/css"
            | "text/javascript"
            | "application/x-javascript"
            | "text/x-typescript"
            | "application/x-typescript"
            | "text/csv"
            | "text/markdown"
            | "text/x-python"
            | "application/x-python-code"
            | "application/json"
            | "text/xml"
            | "application/rtf"
            | "text/rtf"
            // Document formats shown in Gemini examples.
            | "application/pdf"
    )
}
