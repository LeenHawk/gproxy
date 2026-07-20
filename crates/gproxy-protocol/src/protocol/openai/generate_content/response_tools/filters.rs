use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::super::common::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchFilter {
    Comparison(FileSearchComparisonFilter),
    Compound(FileSearchCompoundFilter),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSearchComparisonFilter {
    pub key: String,
    #[serde(rename = "type")]
    pub type_: FileSearchComparisonOperator,
    pub value: FileSearchComparisonValue,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchComparisonValue {
    String(String),
    Number(f64),
    Boolean(bool),
    List(Vec<FileSearchComparisonListValue>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchComparisonListValue {
    String(String),
    Number(f64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileSearchComparisonOperator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
    #[serde(rename = "gt")]
    Gt,
    #[serde(rename = "gte")]
    Gte,
    #[serde(rename = "lt")]
    Lt,
    #[serde(rename = "lte")]
    Lte,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "nin")]
    Nin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSearchCompoundFilter {
    pub filters: Vec<FileSearchFilter>,
    #[serde(rename = "type")]
    pub type_: FileSearchCompoundOperator,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileSearchCompoundOperator {
    #[serde(rename = "and")]
    And,
    #[serde(rename = "or")]
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSearchRankingOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hybrid_search: Option<FileSearchHybridSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranker: Option<FileSearchRanker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSearchHybridSearch {
    pub embedding_weight: f64,
    pub text_weight: f64,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileSearchRanker {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default-2024-11-15")]
    Default20241115,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputerUseEnvironment {
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "mac")]
    Mac,
    #[serde(rename = "linux")]
    Linux,
    #[serde(rename = "ubuntu")]
    Ubuntu,
    #[serde(rename = "browser")]
    Browser,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchUserLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ApproximateLocationType>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpAllowedTools {
    Names(Vec<String>),
    Filter(McpToolFilter),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_names: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpConnectorId {
    #[serde(rename = "connector_dropbox")]
    Dropbox,
    #[serde(rename = "connector_gmail")]
    Gmail,
    #[serde(rename = "connector_googlecalendar")]
    GoogleCalendar,
    #[serde(rename = "connector_googledrive")]
    GoogleDrive,
    #[serde(rename = "connector_microsoftteams")]
    MicrosoftTeams,
    #[serde(rename = "connector_outlookcalendar")]
    OutlookCalendar,
    #[serde(rename = "connector_outlookemail")]
    OutlookEmail,
    #[serde(rename = "connector_sharepoint")]
    SharePoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpRequireApproval {
    Setting(McpApprovalSetting),
    Filter(McpToolApprovalFilter),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpApprovalSetting {
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "never")]
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolApprovalFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always: Option<McpToolFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub never: Option<McpToolFilter>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
