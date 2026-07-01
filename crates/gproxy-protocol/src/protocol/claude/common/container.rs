use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{JsonObject, SkillType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContainerParam {
    Params(ContainerParams),
    Id(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<SkillParam>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillParam {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: SkillType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub expires_at: String,
    pub skills: Vec<Skill>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: SkillType,
    pub version: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}
