use serde::{Deserialize, Serialize};

use super::super::JsonObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditorCodeExecutionViewResultBlock {
    pub content: String,
    pub file_type: TextEditorCodeExecutionFileType,
    #[serde(rename = "type")]
    pub type_: TextEditorCodeExecutionViewResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_lines: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionFileType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "pdf")]
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionViewResultBlockType {
    #[serde(rename = "text_editor_code_execution_view_result")]
    TextEditorCodeExecutionViewResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditorCodeExecutionCreateResultBlock {
    pub is_file_update: bool,
    #[serde(rename = "type")]
    pub type_: TextEditorCodeExecutionCreateResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionCreateResultBlockType {
    #[serde(rename = "text_editor_code_execution_create_result")]
    TextEditorCodeExecutionCreateResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditorCodeExecutionStrReplaceResultBlock {
    #[serde(rename = "type")]
    pub type_: TextEditorCodeExecutionStrReplaceResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_lines: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_lines: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_start: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEditorCodeExecutionStrReplaceResultBlockType {
    #[serde(rename = "text_editor_code_execution_str_replace_result")]
    TextEditorCodeExecutionStrReplaceResult,
}
