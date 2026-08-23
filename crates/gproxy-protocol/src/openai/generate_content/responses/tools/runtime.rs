use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeInterpreterContainer {
    Id(String),
    Auto(CodeInterpreterAutoContainer),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeInterpreterAutoContainer {
    #[serde(rename = "type")]
    pub type_: CodeInterpreterContainerType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<CodeInterpreterMemoryLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<CodeInterpreterNetworkPolicy>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeInterpreterNetworkPolicy {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_secrets: Option<Vec<CodeInterpreterDomainSecret>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeInterpreterDomainSecret {
    pub domain: String,
    pub name: String,
    pub value: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageMask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellEnvironment {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<CodeInterpreterMemoryLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<CodeInterpreterNetworkPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<ResponseShellContainerSkill>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseShellContainerSkill {
    Reference(ResponseShellSkillReference),
    Inline(ResponseShellInlineSkill),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellSkillReference {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellInlineSkill {
    pub description: String,
    pub name: String,
    pub source: ResponseShellInlineSkillSource,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellInlineSkillSource {
    pub data: String,
    pub media_type: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellLocalSkill {
    pub description: String,
    pub name: String,
    pub path: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchContentType {
    Text,
    Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchPreviewUserLocation {
    #[serde(rename = "type")]
    pub type_: ApproximateLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
