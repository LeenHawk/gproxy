use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type JsonValue = Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentRole {
    User,
    Model,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub parts: Vec<Part>,
    /// Must be either 'user' or 'model'.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<ContentRole>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Only one of the data fields (text/inline_data/function_call/function_response/file_data/executable_code/code_execution_result)
    /// should be set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<Blob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_code: Option<ExecutableCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_execution_result: Option<CodeExecutionResult>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    /// Base64-encoded bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    /// Map/Struct-like metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_metadata: Option<JsonValue>,

    /// Only applicable when inline_data or file_data is video.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_metadata: Option<VideoMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    /// The IANA standard MIME type of the source data. Examples: - image/png - image/jpeg
    /// If an unsupported MIME type is provided, an error will be returned.
    pub mime_type: String,
    /// Base64-encoded bytes.
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub response: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<FunctionResponsePart>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub will_continue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling: Option<Scheduling>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponsePart {
    /// Only inline_data is supported here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<FunctionResponseBlob>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponseBlob {
    pub mime_type: String,
    /// Base64-encoded bytes.
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scheduling {
    #[serde(rename = "SCHEDULING_UNSPECIFIED")]
    SchedulingUnspecified,
    #[serde(rename = "SILENT")]
    Silent,
    #[serde(rename = "WHEN_IDLE")]
    WhenIdle,
    #[serde(rename = "INTERRUPT")]
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub file_uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "LANGUAGE_UNSPECIFIED")]
    LanguageUnspecified,
    #[serde(rename = "PYTHON")]
    Python,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableCode {
    pub language: Language,
    pub code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Outcome {
    #[serde(rename = "OUTCOME_UNSPECIFIED")]
    OutcomeUnspecified,
    #[serde(rename = "OUTCOME_OK")]
    OutcomeOk,
    #[serde(rename = "OUTCOME_FAILED")]
    OutcomeFailed,
    #[serde(rename = "OUTCOME_DEADLINE_EXCEEDED")]
    OutcomeDeadlineExceeded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeExecutionResult {
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
}

/// GenerateContentRequest from the generateContent endpoint.
pub type GenerateContentRequest = JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modality {
    #[serde(rename = "MODALITY_UNSPECIFIED")]
    ModalityUnspecified,
    #[serde(rename = "TEXT")]
    Text,
    #[serde(rename = "IMAGE")]
    Image,
    #[serde(rename = "VIDEO")]
    Video,
    #[serde(rename = "AUDIO")]
    Audio,
    #[serde(rename = "DOCUMENT")]
    Document,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModalityTokenCount {
    pub modality: Modality,
    pub token_count: u32,
}
