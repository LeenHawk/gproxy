use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::{AnthropicBetaHeaders, DeletedFileObjectType, FileObjectType, JsonObject};

pub type FileRequestHeaders = AnthropicBetaHeaders;

/// Multipart form fields for `POST /v1/files`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct UploadFileRequestBody {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ListFilesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Repeated `ids[]` query parameters; mutually exclusive with `page` and `limit`.
    #[serde(rename = "ids[]", skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct FilePath {
    pub file_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
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
    /// GA responses include this as either an RFC 3339 timestamp or `null`.
    /// It remains optional so responses selected by the legacy beta header decode.
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

pub type UploadFileResponseBody = FileMetadata;
pub type RetrieveFileResponseBody = FileMetadata;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ListFilesResponseBody {
    pub data: Vec<FileMetadata>,
    pub next_page: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct DeleteFileResponseBody {
    pub id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<DeletedFileObjectType>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

pub type DownloadFileResponseBody = Vec<u8>;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn models_ga_expiration_and_page_cursor_shapes() {
        let file: FileMetadata = serde_json::from_value(json!({
            "id": "file_123",
            "created_at": "2026-08-19T00:00:00Z",
            "filename": "notes.txt",
            "mime_type": "text/plain",
            "size_bytes": 42,
            "type": "file",
            "downloadable": false,
            "expires_at": null
        }))
        .unwrap();
        assert!(file.expires_at.is_none());

        let query = ListFilesQuery::builder()
            .ids(Some(vec!["file_123".into(), "file_456".into()]))
            .build()
            .unwrap();
        assert_eq!(serde_json::to_value(query).unwrap()["ids[]"][1], "file_456");
    }
}
