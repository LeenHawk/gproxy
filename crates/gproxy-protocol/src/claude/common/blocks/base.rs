use serde::{Deserialize, Serialize};

use super::super::{CacheControl, Citation, CitationConfig};
use super::{DocumentSource, ImageSource};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: TextBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<Citation>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TextBlockType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageBlock {
    pub source: ImageSource,
    #[serde(rename = "type")]
    pub type_: ImageBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ImageBlockType {
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentBlock {
    pub source: DocumentSource,
    #[serde(rename = "type")]
    pub type_: DocumentBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DocumentBlockType {
    #[serde(rename = "document")]
    Document,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResultBlock {
    pub content: Vec<TextBlock>,
    pub source: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: SearchResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SearchResultBlockType {
    #[serde(rename = "search_result")]
    SearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub thinking: String,
    #[serde(rename = "type")]
    pub type_: ThinkingBlockType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ThinkingBlockType {
    #[serde(rename = "thinking")]
    Thinking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactedThinkingBlock {
    pub data: String,
    #[serde(rename = "type")]
    pub type_: RedactedThinkingBlockType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RedactedThinkingBlockType {
    #[serde(rename = "redacted_thinking")]
    RedactedThinking,
}
