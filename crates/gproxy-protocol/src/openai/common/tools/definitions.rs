use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::{CustomToolGrammarSyntax, JsonSchema, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionCall {
    pub arguments: String,
    pub name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolInputFormat>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomToolInputFormat {
    Text(CustomToolTextFormat),
    Grammar(CustomToolGrammarFormat),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomToolTextFormat {
    #[serde(rename = "type")]
    pub type_: CustomToolTextFormatType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(CustomToolTextFormatType { Text => "text" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomToolGrammarFormat {
    #[serde(rename = "type")]
    pub type_: CustomToolGrammarFormatType,
    pub definition: String,
    pub syntax: CustomToolGrammarSyntax,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(CustomToolGrammarFormatType { Grammar => "grammar" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomToolGrammar {
    pub definition: String,
    pub syntax: CustomToolGrammarSyntax,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct NamedTool {
    pub name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}
