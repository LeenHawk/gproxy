use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::super::content::ResponseReasoningTextType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum AdditionalToolsRole {
    Unknown,
    User,
    Assistant,
    System,
    Critic,
    Discriminator,
    Developer,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningSummaryPart {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseReasoningSummaryType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseReasoningSummaryType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningTextPart {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseReasoningTextType,
    #[serde(default, flatten)]
    pub rest: Rest,
}
