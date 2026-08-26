//! OpenAI Files wire shapes.
//!
//! The local OpenAI documentation snapshot has no Files control-plane page;
//! these public fields follow the v2 protocol model and its observed service
//! responses. Multipart bytes are represented by their normalized data URL.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{ListObjectType, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateFileRequest {
    pub purpose: FilePurpose,
    pub file: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListFilesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<FilePurpose>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrieveFileRequest {
    pub file_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrieveFileContentRequest {
    pub file_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteFileRequest {
    pub file_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileObject {
    pub id: String,
    pub object: String,
    pub bytes: u64,
    pub created_at: i64,
    pub filename: String,
    pub purpose: FilePurpose,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Evidence gap: `upstream_docs/openai/` has no `/v1/files` schema;
    /// preserve the v2 `FileObject.status_details` payload verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_details: Option<Value>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListFilesResponse {
    pub object: ListObjectType,
    pub data: Vec<FileObject>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteFileResponse {
    pub id: String,
    pub object: String,
    pub deleted: bool,
    #[serde(default, flatten)]
    pub rest: Rest,
}

/// Body bytes returned by `GET /v1/files/{id}/content`.
pub type FileContent = Vec<u8>;

macro_rules! extensible_string {
    ($name:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub enum $name {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant),+
        }
    };
}

extensible_string!(FilePurpose, KnownFilePurpose {
    Assistants => "assistants",
});

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn file_models_round_trip_unknown_fields_and_variants() {
        let value = json!({
            "id":"file_1",
            "object":"file.future",
            "bytes":4,
            "created_at":1,
            "filename":"a.bin",
            "purpose":"future-purpose",
            "status":"future-status",
            "future_file":{"x":1}
        });
        let file: FileObject = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(file).unwrap(), value);

        let list = json!({
            "object":"list",
            "data":[],
            "has_more":false,
            "future_list":true
        });
        let parsed: ListFilesResponse = serde_json::from_value(list.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), list);
    }
}
