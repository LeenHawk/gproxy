use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use time::OffsetDateTime;

use super::types::{AnthropicBeta, AnthropicBetaKnown};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionCreateRequest {
    pub path: SkillVersionCreatePath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skill_version_create_headers"
    )]
    pub headers: Option<SkillVersionCreateHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionCreatePath {
    pub skill_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionCreateHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_skills_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionsListRequest {
    pub path: SkillVersionsListPath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skill_versions_list_headers"
    )]
    pub headers: Option<SkillVersionsListHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<SkillVersionsListQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionsListPath {
    pub skill_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionsListHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_skills_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkillVersionsListQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 1)]
    #[validate(maximum = 1000)]
    pub limit: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionGetRequest {
    pub path: SkillVersionGetPath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skill_version_get_headers"
    )]
    pub headers: Option<SkillVersionGetHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionGetPath {
    pub skill_id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionGetHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_skills_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionDeleteRequest {
    pub path: SkillVersionDeletePath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skill_version_delete_headers"
    )]
    pub headers: Option<SkillVersionDeleteHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionDeletePath {
    pub skill_id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionDeleteHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_skills_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

fn default_skills_beta() -> Option<Vec<AnthropicBeta>> {
    Some(vec![AnthropicBeta::Known(
        AnthropicBetaKnown::Skills20251002,
    )])
}

fn default_skill_version_create_headers() -> Option<SkillVersionCreateHeaders> {
    Some(SkillVersionCreateHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

fn default_skill_versions_list_headers() -> Option<SkillVersionsListHeaders> {
    Some(SkillVersionsListHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

fn default_skill_version_get_headers() -> Option<SkillVersionGetHeaders> {
    Some(SkillVersionGetHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

fn default_skill_version_delete_headers() -> Option<SkillVersionDeleteHeaders> {
    Some(SkillVersionDeleteHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionResource {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub description: String,
    pub directory: String,
    pub name: String,
    pub skill_id: String,
    #[serde(rename = "type")]
    pub object_type: SkillVersionObjectType,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillVersionObjectType {
    #[serde(rename = "skill_version")]
    SkillVersion,
}

pub type SkillVersionCreateResponse = SkillVersionResource;
pub type SkillVersionGetResponse = SkillVersionResource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionsListResponse {
    pub data: Vec<SkillVersionResource>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionDeleteResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub object_type: SkillVersionDeletedObjectType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillVersionDeletedObjectType {
    #[serde(rename = "skill_version_deleted")]
    SkillVersionDeleted,
}
