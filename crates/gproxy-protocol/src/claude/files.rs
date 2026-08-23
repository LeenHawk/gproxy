use serde::{Deserialize, Serialize};

use super::common::{AnthropicBetaHeaders, DeletedFileObjectType, FileObjectType};

pub type FileRequestHeaders = AnthropicBetaHeaders;

/// Multipart form fields for `POST /v1/files`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadFileRequestBody {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListFilesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilePath {
    pub file_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: String,
    pub created_at: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    #[serde(rename = "type")]
    pub type_: FileObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloadable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<FileScope>,
    /// GA responses include this as either an RFC 3339 timestamp or `null`.
    /// It remains optional so responses selected by the legacy beta header decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

pub type UploadFileResponseBody = FileMetadata;
pub type RetrieveFileResponseBody = FileMetadata;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListFilesResponseBody {
    pub data: Vec<FileMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteFileResponseBody {
    pub id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<DeletedFileObjectType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

pub type DownloadFileResponseBody = Vec<u8>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileScope {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: FileScopeType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum FileScopeType {
    Known(KnownFileScopeType),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum KnownFileScopeType {
    #[serde(rename = "session")]
    Session,
}
