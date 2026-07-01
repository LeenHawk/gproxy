use serde::{Deserialize, Serialize};

use super::{Extra, JsonSchema};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatResponseFormat {
    ChatJsonSchema(ChatJsonSchemaFormat),
    Text(TextResponseFormat),
    JsonObject(JsonObjectResponseFormat),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseFormat {
    JsonSchema(JsonSchemaResponseFormat),
    Text(TextResponseFormat),
    JsonObject(JsonObjectResponseFormat),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextResponseFormat {
    #[serde(rename = "type")]
    pub type_: TextResponseFormatType,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextResponseFormatType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatJsonSchemaFormat {
    #[serde(rename = "type")]
    pub type_: JsonSchemaResponseFormatType,
    pub json_schema: JsonSchemaFormat,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonSchemaResponseFormat {
    #[serde(rename = "type")]
    pub type_: JsonSchemaResponseFormatType,
    pub name: String,
    pub schema: JsonSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonSchemaResponseFormatType {
    #[serde(rename = "json_schema")]
    JsonSchema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonObjectResponseFormat {
    #[serde(rename = "type")]
    pub type_: JsonObjectResponseFormatType,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonObjectResponseFormatType {
    #[serde(rename = "json_object")]
    JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonSchemaFormat {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<JsonSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}
