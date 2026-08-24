use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::super::super::ResponseReasoningSummaryPart;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningSummaryPartAddedEvent {
    pub item_id: String,
    pub output_index: u32,
    pub part: ResponseReasoningSummaryPart,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    pub summary_index: u32,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningSummaryPartDoneEvent {
    pub item_id: String,
    pub output_index: u32,
    pub part: ResponseReasoningSummaryPart,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    pub summary_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseReasoningSummaryPartStatus>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseReasoningSummaryPartStatus {
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningSummaryTextDeltaEvent {
    pub delta: String,
    pub item_id: String,
    pub output_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    pub summary_index: u32,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningSummaryTextDoneEvent {
    pub item_id: String,
    pub output_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    pub summary_index: u32,
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}
