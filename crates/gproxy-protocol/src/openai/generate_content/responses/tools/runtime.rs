use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CodeInterpreterNetworkPolicy {
    #[serde(rename = "disabled")]
    Disabled {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "allowlist")]
    Allowlist {
        allowed_domains: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        domain_secrets: Option<Vec<CodeInterpreterDomainSecret>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
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
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseShellEnvironment {
    #[serde(rename = "container_auto")]
    ContainerAuto {
        #[serde(skip_serializing_if = "Option::is_none")]
        file_ids: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        memory_limit: Option<CodeInterpreterMemoryLimit>,
        #[serde(skip_serializing_if = "Option::is_none")]
        network_policy: Option<CodeInterpreterNetworkPolicy>,
        #[serde(skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<ResponseShellContainerSkill>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "local")]
    Local {
        #[serde(skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<ResponseShellLocalSkill>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "container_reference")]
    ContainerReference {
        container_id: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseShellContainerSkill {
    Reference(ResponseShellSkillReference),
    Inline(ResponseShellInlineSkill),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellSkillReference {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseShellSkillReferenceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseShellSkillReferenceType {
    #[serde(rename = "skill_reference")]
    SkillReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellInlineSkill {
    pub description: String,
    pub name: String,
    pub source: ResponseShellInlineSkillSource,
    #[serde(rename = "type")]
    pub type_: ResponseShellInlineSkillType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseShellInlineSkillType {
    #[serde(rename = "inline")]
    Inline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellInlineSkillSource {
    pub data: String,
    pub media_type: ResponseShellInlineSkillMediaType,
    #[serde(rename = "type")]
    pub type_: ResponseShellInlineSkillSourceType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseShellInlineSkillMediaType {
    #[serde(rename = "application/zip")]
    ApplicationZip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseShellInlineSkillSourceType {
    #[serde(rename = "base64")]
    Base64,
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
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
