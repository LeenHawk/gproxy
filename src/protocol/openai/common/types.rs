use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::operation::OperationKey;

use super::model_ids::OpenAiModelId;

pub type Extra = BTreeMap<String, Value>;
pub type JsonSchema = BTreeMap<String, Value>;
pub type LogitBias = BTreeMap<String, f64>;
pub type Metadata = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModerationConfig {
    pub model: OpenAiModelId,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModerationResult {
    pub categories: BTreeMap<String, bool>,
    pub category_applied_input_types: BTreeMap<String, Vec<ModerationInputType>>,
    pub category_scores: BTreeMap<String, f64>,
    pub flagged: bool,
    pub model: OpenAiModelId,
    #[serde(rename = "type")]
    pub type_: ModerationResultType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationInputType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationResultType {
    #[serde(rename = "moderation_result")]
    ModerationResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModerationError {
    pub code: String,
    pub message: String,
    #[serde(rename = "type")]
    pub type_: ModerationErrorType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationErrorType {
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAiWireModel<TRequest, TResponse> {
    pub operation_key: OperationKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<TRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<TResponse>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrList {
    String(String),
    List(Vec<String>),
}
