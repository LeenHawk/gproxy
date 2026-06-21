use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod blocks;
pub mod cache;
pub mod container;
pub mod context_management;
pub mod diagnostics;
pub mod errors;
pub mod fallback;
pub mod mcp;
pub mod output;
pub mod server_tools;
pub mod stop;
pub mod thinking;
pub mod tools;
pub mod types;
pub mod usage;

pub use blocks::*;
pub use cache::*;
pub use container::*;
pub use context_management::*;
pub use diagnostics::*;
pub use errors::*;
pub use fallback::*;
pub use mcp::*;
pub use output::*;
pub use server_tools::*;
pub use stop::*;
pub use thinking::*;
pub use tools::*;
pub use types::*;
pub use usage::*;

pub type JsonObject = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AnthropicBetaHeaders {
    #[serde(
        rename = "anthropic-beta",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrArray<T> {
    String(String),
    Array(Vec<T>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoolOrStringArray {
    Bool(bool),
    Array(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedObject {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Citation {
    #[serde(rename = "type")]
    pub type_: CitationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cited_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_char_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_char_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_page_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_page_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_block_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_block_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_result_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_index: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonSchemaFormat {
    #[serde(rename = "type")]
    pub type_: JsonSchemaFormatType,
    pub schema: JsonObject,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}
