use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{JsonSchema, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatResponseFormat {
    JsonSchema(ChatJsonSchemaFormat),
    Text(TextResponseFormat),
    JsonObject(JsonObjectResponseFormat),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseFormat {
    JsonSchema(JsonSchemaResponseFormat),
    Text(TextResponseFormat),
    JsonObject(JsonObjectResponseFormat),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextResponseFormat {
    #[serde(rename = "type")]
    pub type_: TextResponseFormatType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(TextResponseFormatType { Text => "text" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatJsonSchemaFormat {
    #[serde(rename = "type")]
    pub type_: JsonSchemaResponseFormatType,
    pub json_schema: JsonSchemaFormat,
    #[serde(default, flatten)]
    pub rest: Rest,
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
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(JsonSchemaResponseFormatType { JsonSchema => "json_schema" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonObjectResponseFormat {
    #[serde(rename = "type")]
    pub type_: JsonObjectResponseFormatType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(JsonObjectResponseFormatType { JsonObject => "json_object" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonSchemaFormat {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<JsonSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
