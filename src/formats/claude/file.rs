use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use time::OffsetDateTime;

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_files_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

fn default_files_beta() -> Option<Vec<AnthropicBeta>> {
    Some(vec![AnthropicBeta::Known(
        AnthropicBetaKnown::FilesApi20250414,
    )])
}

fn default_files_headers() -> Option<FilesHeaders> {
    Some(FilesHeaders {
        anthropic_beta: default_files_beta(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadRequest {
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_files_headers"
    )]
    pub headers: Option<FilesHeaders>,
}

pub type FileUploadResponse = FileMetadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesListRequest {
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_files_headers"
    )]
    pub headers: Option<FilesHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<FilesListQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FilesListQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 1)]
    #[validate(maximum = 1000)]
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesListResponse {
    pub data: Vec<FileMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileGetRequest {
    pub path: FilePath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_files_headers"
    )]
    pub headers: Option<FilesHeaders>,
}

pub type FileGetResponse = FileMetadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDownloadRequest {
    pub path: FilePath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_files_headers"
    )]
    pub headers: Option<FilesHeaders>,
}

pub type FileDownloadResponse = Vec<u8>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDeleteRequest {
    pub path: FilePath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_files_headers"
    )]
    pub headers: Option<FilesHeaders>,
}

pub type FileDeleteResponse = DeletedFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePath {
    pub file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    #[serde(rename = "type")]
    pub object_type: FileObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloadable: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FileObjectType {
    #[serde(rename = "file")]
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedFile {
    pub id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub object_type: Option<DeletedFileObjectType>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DeletedFileObjectType {
    #[serde(rename = "file_deleted")]
    FileDeleted,
}
