use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeFunctionArgumentsDeltaEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub call_id: String,
    pub delta: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeFunctionArgumentsDoneEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub call_id: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeMcpArgumentsDeltaEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub delta: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeMcpArgumentsDoneEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub arguments: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeMcpCallStatusEvent {
    pub item_id: String,
    pub output_index: u32,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeMcpListToolsStatusEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
