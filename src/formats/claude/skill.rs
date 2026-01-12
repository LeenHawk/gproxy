use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use time::OffsetDateTime;

use super::types::{AnthropicBeta, AnthropicBetaKnown};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsCreateRequest {
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skills_create_headers"
    )]
    pub headers: Option<SkillsCreateHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsCreateHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_skills_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsListRequest {
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skills_list_headers"
    )]
    pub headers: Option<SkillsListHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<SkillsListQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsListHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_skills_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkillsListQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(maximum = 100)]
    pub limit: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillGetRequest {
    pub path: SkillGetPath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skill_get_headers"
    )]
    pub headers: Option<SkillGetHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillGetPath {
    pub skill_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillGetHeaders {
    #[serde(
        rename = "anthropic-beta",
        skip_serializing_if = "Option::is_none",
        default = "default_skills_beta"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDeleteRequest {
    pub path: SkillDeletePath,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default = "default_skill_delete_headers"
    )]
    pub headers: Option<SkillDeleteHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDeletePath {
    pub skill_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDeleteHeaders {
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

fn default_skills_create_headers() -> Option<SkillsCreateHeaders> {
    Some(SkillsCreateHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

fn default_skills_list_headers() -> Option<SkillsListHeaders> {
    Some(SkillsListHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

fn default_skill_get_headers() -> Option<SkillGetHeaders> {
    Some(SkillGetHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

fn default_skill_delete_headers() -> Option<SkillDeleteHeaders> {
    Some(SkillDeleteHeaders {
        anthropic_beta: default_skills_beta(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResource {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub display_title: String,
    pub latest_version: String,
    pub source: SkillSource,
    #[serde(rename = "type")]
    pub object_type: SkillObjectType,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillSource {
    #[serde(rename = "custom")]
    Custom,
    #[serde(rename = "anthropic")]
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillObjectType {
    #[serde(rename = "skill")]
    Skill,
}

pub type SkillCreateResponse = SkillResource;
pub type SkillGetResponse = SkillResource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsListResponse {
    pub data: Vec<SkillResource>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDeleteResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub object_type: SkillDeletedObjectType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillDeletedObjectType {
    #[serde(rename = "skill_deleted")]
    SkillDeleted,
}
