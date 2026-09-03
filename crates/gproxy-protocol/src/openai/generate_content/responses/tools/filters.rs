use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{ApproximateLocationType, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchFilter {
    Comparison(FileSearchComparisonFilter),
    Compound(FileSearchCompoundFilter),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearchComparisonFilter {
    pub key: String,
    #[serde(rename = "type")]
    pub type_: FileSearchComparisonOperator,
    pub value: FileSearchComparisonValue,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchComparisonValue {
    String(String),
    Number(f64),
    Boolean(bool),
    List(Vec<FileSearchComparisonListValue>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchComparisonListValue {
    String(String),
    Number(f64),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSearchComparisonOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Nin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearchCompoundFilter {
    pub filters: Vec<FileSearchFilter>,
    #[serde(rename = "type")]
    pub type_: FileSearchCompoundOperator,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSearchCompoundOperator {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearchRankingOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hybrid_search: Option<FileSearchHybridSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranker: Option<FileSearchRanker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearchHybridSearch {
    pub embedding_weight: f64,
    pub text_weight: f64,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileSearchRanker {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default-2024-11-15")]
    Default20241115,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerUseEnvironment {
    Windows,
    Mac,
    Linux,
    Ubuntu,
    Browser,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpAllowedTools {
    Names(Vec<String>),
    Filter(McpToolFilter),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct McpToolFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_names: Option<Vec<String>>,
    #[serde(default, flatten)]
    pub rest: Rest,
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
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpApprovalSetting {
    Always,
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct McpToolApprovalFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always: Option<McpToolFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub never: Option<McpToolFilter>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
